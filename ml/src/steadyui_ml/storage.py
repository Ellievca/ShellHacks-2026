"""Append-only JSONL persistence for interaction records."""

from __future__ import annotations

import json
from pathlib import Path

from .schema import InteractionRecord


def append_record(path: str | Path, record: InteractionRecord) -> None:
    """Append one fully materialized interaction record to a JSONL batch."""
    if record.features is None:
        raise ValueError("calculate and attach features before persisting a record")
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("a", encoding="utf-8") as file:
        json.dump(record.to_dict(), file, separators=(",", ":"))
        file.write("\n")
