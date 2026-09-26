# SteadyUI ML

This package owns the versioned contract between cursor collection and model training. Each line in a JSONL file is one complete `InteractionRecord`: the target shown to a person, the ordered cursor samples leading to it, its outcome label, and a reproducible set of movement features.

## Set up

From this directory, create an environment and install the training dependencies:

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -e '.[dev]'
pytest
```

## Storage layout

```text
ml/
  data/
    raw/        # append-only JSONL interaction captures; ignored by Git
    processed/  # feature tables / train-validation splits; ignored by Git
  models/       # trained artifacts and metadata; ignored by Git
  src/steadyui_ml/
    schema.py   # stable, serializable capture contract
    features.py # deterministic feature extraction
    ingestion.py # validates browser payloads and calculates features
    storage.py   # append-only JSONL read/write helpers
    datasets.py  # leakage-safe train/test preparation
```

Use one file per collection batch, for example `data/raw/2026-09-26-session-a.jsonl`. Never store direct page text, form values, or other sensitive content in a record. `page_id` should be an application-controlled opaque or hashed identifier.

## Plain-language glossary

This project is designed to learn from cursor movement. The extension collects the raw events in the browser; this Python package gives those events a consistent shape and prepares them for future model training.

- **Cursor sample** — One snapshot of the cursor: its horizontal position (`x`), vertical position (`y`), time, and optionally velocity. A sequence of samples describes one cursor path.
- **Target** — The on-screen item a person is expected or trying to reach, such as a button, link, or input. It has a stable identifier, a type, and a viewport-relative rectangle (`x`, `y`, width, height).
- **Interaction** or **attempt** — One complete cursor-to-target event: movement starts, the cursor moves, and the event ends with a click, a miss, a timeout, or abandonment. Each complete interaction becomes one training record.
- **Label** — The known result attached to an interaction. Labels give a future ML model the answers from which to learn. The current outcomes are `success` (intended target clicked), `miss` (a different location clicked), `abandoned` (the attempt ended without the intended click), and `unknown` (the outcome could not be determined).
- **Feature** — A consistent numeric summary calculated from the raw samples. Cursor paths may have different numbers of points, but every interaction can have the same feature columns. That makes the data suitable for a model.
- **Raw data** — Original captured samples and event details. It should be retained unchanged in `data/raw/`.
- **Processed data** — Validated and transformed records, commonly a table with one interaction per row and one calculated feature per column. It belongs in `data/processed/`.
- **Training** — Giving a model many examples of features together with labels so it can learn patterns associated with each outcome.
- **Model** — The saved result of training. Later, it could estimate the likelihood of a miss from the features of a new cursor attempt.
- **JSONL** — “JSON Lines”: a text format with one complete JSON interaction record per line. It makes it easy to append a new capture without rewriting an entire dataset.

### Current movement features

The feature calculator produces these values for each interaction:

- `sample_count`: number of recorded cursor points.
- `duration_ms`: elapsed time from the first to last sample, in milliseconds.
- `path_length_px`: total distance travelled along the cursor route, in CSS pixels.
- `displacement_px`: direct straight-line distance from the first to the last sample.
- `straightness`: displacement divided by path length. A value near `1` indicates a direct route; lower values indicate a more indirect route.
- `mean_speed_px_s` and `max_speed_px_s`: average and fastest cursor speed, in pixels per second.
- `speed_std_px_s`: how much the cursor speed varied.
- `mean_acceleration_px_s2` and `max_acceleration_px_s2`: average and largest changes in speed, in pixels per second squared.
- `direction_changes`: number of substantial turns (a change of more than 45 degrees between adjacent movement segments).
- `final_target_distance_px`: distance between the final cursor position and the center of the target.

## Extension payload contract

The existing content script's `{x, y, time, velocity}` objects map directly through `CursorSample.from_browser_event()`. `time` is a monotonic timestamp in milliseconds (`performance.now()`), and velocity is pixels per second. On a target interaction, send the samples in chronological order with the target and a post-interaction label. The backend can then call `InteractionRecord.with_calculated_features()` before appending the record.

```python
from steadyui_ml.schema import BoundingBox, CursorSample, InteractionLabel, InteractionRecord, Target
from steadyui_ml.storage import append_record

record = InteractionRecord(
    interaction_id="8ea0a70e-40f7-4bda-a70e-40f7b4daa011",
    session_id="session-opaque-id",
    page_id="sha256:...",
    target=Target("continue-button", "button", BoundingBox(840, 620, 140, 44)),
    samples=(
        CursorSample(x=600, y=520, time_ms=1000),
        CursorSample(x=730, y=580, time_ms=1100),
    ),
    label=InteractionLabel(outcome="success", source="instrumentation"),
).with_calculated_features()

append_record("data/raw/collection.jsonl", record)
```

## Collection pipeline

Use the framework-neutral receive functions at the boundary of an HTTP handler,
message consumer, or local collector. They validate the payload, calculate the
trajectory features on receipt, and append exactly one JSONL line only after the
record is complete.

```python
from steadyui_ml import receive_calibration_trial, receive_interaction

# `trajectory_payload` uses the browser sample shape: {x, y, time, velocity}.
record = receive_interaction(trajectory_payload, "data/raw/trajectories.jsonl")

# Keep calibration measurements separate from labelled model examples.
trial = receive_calibration_trial(
    {
        "trial_id": "target-1",
        "session_id": "session-opaque-id",
        "target_x": 400,
        "target_y": 300,
        "pointer_x": 403,
        "pointer_y": 298,
        "time_ms": 1550,
    },
    "data/raw/calibration.jsonl",
)
```

The receive call is the acknowledgement boundary: return a successful response to
the client only after it returns. Calibration and trajectory files are separate so
calibration offsets cannot be mistaken for outcome labels. Use `read_records()`,
`feature_rows()`, and `split_by_session()` to build model input and a deterministic
train/test split without putting one session in both sets.

### Required fields

- `CursorSample`: `x`, `y`, `time_ms`; `velocity_px_s` is retained when provided, otherwise calculated.
- `Target`: a stable `target_id`, target type, and viewport-relative bounding box. Its center is used for target-distance features.
- `InteractionLabel`: `outcome` (`success`, `miss`, `abandoned`, or `unknown`) and its source. Human labels can add `intent` and `confidence`.
- `MovementFeatures`: generated only from the ordered sample sequence and target; it includes timing, distance, speed, acceleration, straightness, direction changes, and final target distance.

`schema_version` is written with every record. Make a new schema version rather than silently changing meanings or units.
