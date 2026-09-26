from math import isclose

import pytest

from steadyui_ml import (
    BoundingBox,
    CursorSample,
    InteractionLabel,
    InteractionRecord,
    Target,
    calculate_movement_features,
)


def test_features_for_a_straight_line_to_target() -> None:
    target = Target("primary-action", "button", BoundingBox(95, -5, 10, 10))
    samples = (
        CursorSample(0, 0, 0),
        CursorSample(30, 40, 100),
        CursorSample(60, 80, 200),
        CursorSample(100, 0, 300),
    )

    features = calculate_movement_features(samples, target)

    assert features.sample_count == 4
    assert features.duration_ms == 300
    assert isclose(features.path_length_px, 50 + 50 + 89.4427191, rel_tol=1e-7)
    assert features.displacement_px == 100
    assert features.max_speed_px_s == pytest.approx(894.427191)
    assert features.final_target_distance_px == 0


def test_record_rejects_out_of_order_samples() -> None:
    with pytest.raises(ValueError, match="strictly increasing"):
        InteractionRecord(
            interaction_id="one",
            session_id="session",
            page_id="page",
            target=Target("cta", "button", BoundingBox(0, 0, 10, 10)),
            samples=(CursorSample(0, 0, 20), CursorSample(1, 1, 20)),
            label=InteractionLabel("unknown", "instrumentation"),
        )
