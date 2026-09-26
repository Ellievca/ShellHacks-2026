"""Versioned cursor-trajectory contracts and feature extraction."""

from .features import calculate_movement_features
from .schema import (
    BoundingBox,
    CursorSample,
    InteractionLabel,
    InteractionRecord,
    MovementFeatures,
    Target,
)

__all__ = [
    "BoundingBox",
    "CursorSample",
    "InteractionLabel",
    "InteractionRecord",
    "MovementFeatures",
    "Target",
    "calculate_movement_features",
]
