"""Boundary functions for accepting browser collection payloads.

The functions here are deliberately framework-neutral.  A web handler can pass its
decoded JSON body to them and return success only after the append completes.
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path
from typing import Any

from .schema import CalibrationTrial, CursorSample, InteractionLabel, InteractionRecord, Target
from .storage import append_calibration_trial, append_record


def _require_mapping(value: object, field: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise ValueError(f"{field} must be an object")
    return value


def _sample_from_payload(value: object) -> CursorSample:
    sample = _require_mapping(value, "sample")
    # The extension emits ``time`` and ``velocity``; stored records use explicit
    # unit-bearing names. Supporting both keeps HTTP ingestion and re-ingestion
    # of exported JSONL deterministic.
    if "time_ms" in sample:
        return CursorSample.from_dict(sample)
    try:
        return CursorSample.from_browser_event(dict(sample))
    except KeyError as error:
        raise ValueError(f"sample is missing {error.args[0]!r}") from error


def interaction_from_payload(payload: Mapping[str, Any]) -> InteractionRecord:
    """Validate a browser payload and materialize its derived features."""
    try:
        target = Target.from_dict(_require_mapping(payload["target"], "target"))
        label = InteractionLabel.from_dict(_require_mapping(payload["label"], "label"))
        raw_samples = payload["samples"]
    except KeyError as error:
        raise ValueError(f"interaction payload is missing {error.args[0]!r}") from error
    if isinstance(raw_samples, (str, bytes)) or not hasattr(raw_samples, "__iter__"):
        raise ValueError("samples must be an array")

    try:
        record = InteractionRecord(
            interaction_id=payload["interaction_id"],
            session_id=payload["session_id"],
            page_id=payload["page_id"],
            target=target,
            samples=tuple(_sample_from_payload(sample) for sample in raw_samples),
            label=label,
            schema_version=payload.get("schema_version", "1.0"),
        )
    except KeyError as error:
        raise ValueError(f"interaction payload is missing {error.args[0]!r}") from error
    return record.with_calculated_features()


def calibration_trial_from_payload(payload: Mapping[str, Any]) -> CalibrationTrial:
    """Validate a calibration-trial payload from the browser client."""
    try:
        return CalibrationTrial.from_dict(payload)
    except KeyError as error:
        raise ValueError(f"calibration payload is missing {error.args[0]!r}") from error


def receive_interaction(payload: Mapping[str, Any], destination: str | Path) -> InteractionRecord:
    """Validate, feature-engineer, and durably append one labelled trajectory."""
    record = interaction_from_payload(payload)
    append_record(destination, record)
    return record


def receive_calibration_trial(
    payload: Mapping[str, Any], destination: str | Path
) -> CalibrationTrial:
    """Validate and durably append one calibration measurement."""
    trial = calibration_trial_from_payload(payload)
    append_calibration_trial(destination, trial)
    return trial
