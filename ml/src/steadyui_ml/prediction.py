"""Load and use the active local trajectory-outcome model."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import joblib

from .ingestion import features_from_payload
from .training import FEATURE_COLUMNS


def predict(payload: dict[str, Any], model_path: str | Path) -> dict[str, object]:
    """Return outcome probabilities for a browser-shaped target trajectory."""
    features = features_from_payload(payload)
    model = joblib.load(model_path)
    values = [[float(getattr(features, column)) for column in FEATURE_COLUMNS]]
    probabilities = model.predict_proba(values)[0]
    return {
        "prediction": str(model.predict(values)[0]),
        "probabilities": {
            str(label): float(probability)
            for label, probability in zip(model.classes_, probabilities, strict=True)
        },
        "features": features.__dict__,
    }
