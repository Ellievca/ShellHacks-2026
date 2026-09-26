"""Local supervised training for labelled pointer trajectories."""

from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass, fields
from pathlib import Path
from tempfile import NamedTemporaryFile
from typing import Any

import joblib
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, classification_report
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

from .datasets import feature_rows, split_by_session
from .schema import MovementFeatures
from .storage import read_records

FEATURE_COLUMNS = tuple(field.name for field in fields(MovementFeatures))


@dataclass(frozen=True)
class TrainingResult:
    training_examples: int
    test_examples: int
    labels: tuple[str, ...]
    accuracy: float
    report: dict[str, Any]


def _write_json_atomically(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as file:
        json.dump(payload, file, indent=2, sort_keys=True)
        temporary_path = Path(file.name)
    os.replace(temporary_path, path)


def train_model(
    raw_path: str | Path,
    model_path: str | Path,
    metadata_path: str | Path | None = None,
    *,
    test_fraction: float = 0.2,
    seed: int = 0,
) -> TrainingResult:
    """Train and atomically save a local outcome classifier.

    ``unknown`` records are retained in raw storage but excluded because they do
    not supply a supervised answer. Sessions are kept wholly in train or test.
    """
    records = tuple(record for record in read_records(raw_path) if record.label.outcome != "unknown")
    train, test = split_by_session(records, test_fraction=test_fraction, seed=seed)
    train_rows = tuple(feature_rows(train))
    test_rows = tuple(feature_rows(test))
    train_labels = [str(row["outcome"]) for row in train_rows]
    if len(set(train_labels)) < 2:
        raise ValueError("training data must contain at least two outcome labels")

    def matrix(rows: tuple[dict[str, object], ...]) -> list[list[float]]:
        return [[float(row[column]) for column in FEATURE_COLUMNS] for row in rows]

    model = Pipeline(
        [
            ("scale", StandardScaler()),
            ("classifier", LogisticRegression(max_iter=1_000, random_state=seed)),
        ]
    )
    model.fit(matrix(train_rows), train_labels)
    predictions = model.predict(matrix(test_rows))
    result = TrainingResult(
        training_examples=len(train_rows),
        test_examples=len(test_rows),
        labels=tuple(sorted(set(train_labels))),
        accuracy=float(accuracy_score([str(row["outcome"]) for row in test_rows], predictions)),
        report=classification_report(
            [str(row["outcome"]) for row in test_rows], predictions, output_dict=True, zero_division=0
        ),
    )

    destination = Path(model_path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile("wb", dir=destination.parent, delete=False) as file:
        joblib.dump(model, file)
        temporary_path = Path(file.name)
    os.replace(temporary_path, destination)

    metadata_destination = (
        Path(metadata_path)
        if metadata_path is not None
        else destination.with_suffix(destination.suffix + ".metadata.json")
    )
    _write_json_atomically(
        metadata_destination,
        {
            "feature_columns": FEATURE_COLUMNS,
            "model_type": "standard-scaled logistic regression",
            "schema_version": "1.0",
            "test_fraction": test_fraction,
            "seed": seed,
            "metrics": asdict(result),
        },
    )
    return result
