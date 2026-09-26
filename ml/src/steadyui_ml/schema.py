"""JSON-serializable, versioned records used for cursor-model training."""

from __future__ import annotations

from dataclasses import asdict, dataclass, replace
from itertools import pairwise
from typing import Literal

SCHEMA_VERSION = "1.0"
Outcome = Literal["success", "miss", "abandoned", "unknown"]
LabelSource = Literal["instrumentation", "human", "inferred"]


@dataclass(frozen=True)
class BoundingBox:
    """Viewport-relative CSS-pixel rectangle for the intended target."""

    x: float
    y: float
    width: float
    height: float

    def __post_init__(self) -> None:
        if self.width <= 0 or self.height <= 0:
            raise ValueError("target bounding-box width and height must be positive")

    @property
    def center(self) -> tuple[float, float]:
        return (self.x + self.width / 2, self.y + self.height / 2)


@dataclass(frozen=True)
class Target:
    """Stable target identity and geometry, without page content."""

    target_id: str
    target_type: str
    bounds: BoundingBox

    def __post_init__(self) -> None:
        if not self.target_id:
            raise ValueError("target_id is required")


@dataclass(frozen=True)
class CursorSample:
    """One ordered cursor observation; time is monotonic milliseconds."""

    x: float
    y: float
    time_ms: float
    velocity_px_s: float | None = None

    def __post_init__(self) -> None:
        if self.velocity_px_s is not None and self.velocity_px_s < 0:
            raise ValueError("velocity_px_s cannot be negative")

    @classmethod
    def from_browser_event(cls, event: dict[str, float]) -> CursorSample:
        """Adapt the extension's current `{x, y, time, velocity}` payload."""
        return cls(
            x=event["x"],
            y=event["y"],
            time_ms=event["time"],
            velocity_px_s=event.get("velocity"),
        )


@dataclass(frozen=True)
class InteractionLabel:
    """Observed or annotated result for a target-directed cursor movement."""

    outcome: Outcome
    source: LabelSource
    intent: str | None = None
    confidence: float | None = None

    def __post_init__(self) -> None:
        if self.outcome not in {"success", "miss", "abandoned", "unknown"}:
            raise ValueError("outcome must be success, miss, abandoned, or unknown")
        if self.source not in {"instrumentation", "human", "inferred"}:
            raise ValueError("source must be instrumentation, human, or inferred")
        if self.confidence is not None and not 0 <= self.confidence <= 1:
            raise ValueError("confidence must be between 0 and 1")


@dataclass(frozen=True)
class MovementFeatures:
    """Derived quantities. Units are encoded in field names where applicable."""

    sample_count: int
    duration_ms: float
    path_length_px: float
    displacement_px: float
    straightness: float
    mean_speed_px_s: float
    max_speed_px_s: float
    speed_std_px_s: float
    mean_acceleration_px_s2: float
    max_acceleration_px_s2: float
    direction_changes: int
    final_target_distance_px: float


@dataclass(frozen=True)
class InteractionRecord:
    """Atomic training example persisted as exactly one JSONL object."""

    interaction_id: str
    session_id: str
    page_id: str
    target: Target
    samples: tuple[CursorSample, ...]
    label: InteractionLabel
    features: MovementFeatures | None = None
    schema_version: str = SCHEMA_VERSION

    def __post_init__(self) -> None:
        if not self.interaction_id or not self.session_id or not self.page_id:
            raise ValueError("interaction_id, session_id, and page_id are required")
        if not self.samples:
            raise ValueError("at least one cursor sample is required")
        times = [sample.time_ms for sample in self.samples]
        if any(current <= previous for previous, current in pairwise(times)):
            raise ValueError("cursor samples must have strictly increasing time_ms")

    def with_calculated_features(self) -> InteractionRecord:
        from .features import calculate_movement_features

        return replace(self, features=calculate_movement_features(self.samples, self.target))

    def to_dict(self) -> dict[str, object]:
        """Return a JSON-ready representation with nested named fields."""
        return asdict(self)
