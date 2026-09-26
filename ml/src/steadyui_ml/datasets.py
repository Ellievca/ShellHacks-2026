"""Leakage-safe conversion of persisted trajectories into model rows."""

from __future__ import annotations

import random
from collections.abc import Iterable, Iterator

from .schema import InteractionRecord


def feature_rows(records: Iterable[InteractionRecord]) -> Iterator[dict[str, object]]:
    """Yield one flat, labelled row per interaction for a tabular model.

    Raw samples remain in the raw batch.  Keeping only the stable interaction ID,
    session ID, feature values, and label in these rows prevents page content or
    target text from becoming accidental model inputs.
    """
    for record in records:
        features = record.features
        if features is None:
            raise ValueError(f"record {record.interaction_id!r} has no calculated features")
        row: dict[str, object] = {
            "interaction_id": record.interaction_id,
            "session_id": record.session_id,
            "outcome": record.label.outcome,
        }
        row.update(features.__dict__)
        yield row


def split_by_session(
    records: Iterable[InteractionRecord], test_fraction: float = 0.2, seed: int = 0
) -> tuple[tuple[InteractionRecord, ...], tuple[InteractionRecord, ...]]:
    """Split examples by session, so one person's session cannot leak across sets."""
    if not 0 < test_fraction < 1:
        raise ValueError("test_fraction must be between 0 and 1")
    materialized = tuple(records)
    sessions = sorted({record.session_id for record in materialized})
    if len(sessions) < 2:
        raise ValueError("at least two sessions are required for a train/test split")

    random.Random(seed).shuffle(sessions)
    test_count = max(1, min(len(sessions) - 1, round(len(sessions) * test_fraction)))
    test_sessions = set(sessions[:test_count])
    test = tuple(record for record in materialized if record.session_id in test_sessions)
    train = tuple(record for record in materialized if record.session_id not in test_sessions)
    return train, test
