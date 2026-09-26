"""Append-only, recoverable JSONL persistence for collection records."""

from __future__ import annotations

import json
from pathlib import Path

from collections.abc import Callable, Iterator
from typing import Any, TypeVar

from .schema import CalibrationTrial, InteractionRecord

T = TypeVar("T")


def _append_json_line(path: str | Path, payload: dict[str, object]) -> None:
    """Append one compact JSON object as one complete line.

    Flushing before unlock makes an acknowledged collection event immediately
    readable by a training job. ``flock`` protects against interleaved writes by
    concurrent workers on POSIX hosts.
    """
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("a", encoding="utf-8") as file:
        try:
            import fcntl

            fcntl.flock(file.fileno(), fcntl.LOCK_EX)
        except ImportError:  # pragma: no cover - Windows has no fcntl
            fcntl = None
        try:
            json.dump(payload, file, separators=(",", ":"), allow_nan=False)
            file.write("\n")
            file.flush()
        finally:
            if fcntl is not None:
                fcntl.flock(file.fileno(), fcntl.LOCK_UN)


def append_record(path: str | Path, record: InteractionRecord) -> None:
    """Append one feature-complete labelled trajectory to a JSONL batch."""
    if record.features is None:
        raise ValueError("calculate and attach features before persisting a record")
    _append_json_line(path, record.to_dict())


def append_calibration_trial(path: str | Path, trial: CalibrationTrial) -> None:
    """Append one calibration measurement to its own JSONL batch."""
    _append_json_line(path, trial.to_dict())


def _read_jsonl(path: str | Path, factory: Callable[[dict[str, Any]], T]) -> Iterator[T]:
    source = Path(path)
    with source.open(encoding="utf-8") as file:
        for line_number, line in enumerate(file, start=1):
            if not line.strip():
                continue
            try:
                payload = json.loads(line)
                if not isinstance(payload, dict):
                    raise ValueError("JSONL entries must be objects")
                yield factory(payload)
            except (json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
                message = f"invalid record in {source} at line {line_number}: {error}"
                raise ValueError(message) from error


def read_records(path: str | Path) -> Iterator[InteractionRecord]:
    """Yield validated labelled trajectories from a JSONL batch."""
    return _read_jsonl(path, InteractionRecord.from_dict)


def read_calibration_trials(path: str | Path) -> Iterator[CalibrationTrial]:
    """Yield validated calibration measurements from a JSONL batch."""
    return _read_jsonl(path, CalibrationTrial.from_dict)
