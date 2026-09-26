"""Versioned cursor-trajectory contracts and feature extraction."""

from .datasets import feature_rows, split_by_session
from .features import calculate_movement_features
from .ingestion import (
    calibration_trial_from_payload,
    interaction_from_payload,
    receive_calibration_trial,
    receive_interaction,
)
from .schema import (
    BoundingBox,
    CalibrationTrial,
    CursorSample,
    InteractionLabel,
    InteractionRecord,
    MovementFeatures,
    Target,
)
from .storage import (
    append_calibration_trial,
    append_record,
    read_calibration_trials,
    read_records,
)

__all__ = [
    "BoundingBox",
    "CalibrationTrial",
    "CursorSample",
    "InteractionLabel",
    "InteractionRecord",
    "MovementFeatures",
    "Target",
    "append_calibration_trial",
    "append_record",
    "calibration_trial_from_payload",
    "calculate_movement_features",
    "feature_rows",
    "interaction_from_payload",
    "read_calibration_trials",
    "read_records",
    "receive_calibration_trial",
    "receive_interaction",
    "split_by_session",
]
