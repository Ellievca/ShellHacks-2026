from steadyui_ml import (
    feature_rows,
    interaction_from_payload,
    read_calibration_trials,
    read_records,
    receive_calibration_trial,
    receive_interaction,
    split_by_session,
)


def _payload(session_id: str = "session-a") -> dict[str, object]:
    return {
        "interaction_id": f"interaction-{session_id}",
        "session_id": session_id,
        "page_id": "page-opaque-id",
        "target": {
            "target_id": "continue",
            "target_type": "button",
            "bounds": {"x": 90, "y": 0, "width": 20, "height": 20},
        },
        "samples": [
            {"x": 0, "y": 0, "time": 1},
            {"x": 100, "y": 10, "time": 101, "velocity": 1004.987562},
        ],
        "label": {"outcome": "success", "source": "instrumentation"},
    }


def test_receive_interaction_calculates_and_round_trips_jsonl(tmp_path) -> None:
    path = tmp_path / "raw" / "interactions.jsonl"

    received = receive_interaction(_payload(), path)

    assert received.features is not None
    assert list(read_records(path)) == [received]


def test_receive_calibration_trial_round_trips_jsonl(tmp_path) -> None:
    path = tmp_path / "raw" / "calibration.jsonl"
    payload = {
        "trial_id": "cal-1",
        "session_id": "session-a",
        "target_x": 100,
        "target_y": 200,
        "pointer_x": 104,
        "pointer_y": 197,
        "time_ms": 123.5,
    }

    received = receive_calibration_trial(payload, path)

    assert received.offset_x_px == 4
    assert received.offset_y_px == -3
    assert list(read_calibration_trials(path)) == [received]


def test_feature_rows_and_session_split_keep_sessions_separate() -> None:
    first = interaction_from_payload(_payload("session-a"))
    # Construct another feature-complete record without relying on storage contents.
    second = first.__class__(
        interaction_id="interaction-session-b",
        session_id="session-b",
        page_id=first.page_id,
        target=first.target,
        samples=first.samples,
        label=first.label,
        features=first.features,
    )

    train, test = split_by_session((first, second), test_fraction=0.5)

    assert {record.session_id for record in train}.isdisjoint(
        record.session_id for record in test
    )
    assert next(feature_rows((first,)))["outcome"] == "success"
