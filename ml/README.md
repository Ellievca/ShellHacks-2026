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

## How to use the local companion

This is the shortest end-to-end workflow for the current repository. The
companion must be running before the browser extension can save data to `ml/`.

### One-time setup

1. Run the [Set up](#set-up) commands above.
2. Generate a private token:

   ```bash
   python -c 'import secrets; print(secrets.token_urlsafe(32))'
   ```

3. In Chrome, open `chrome://extensions`, enable **Developer mode**, select
   **Load unpacked**, and choose `<repository>/extension/public`.
4. Open the SteadyUI extension's **Details** page, choose **Extension options**,
   and enter `http://127.0.0.1:8765` plus the generated token. Save it.

### Each time you collect data

1. Start the local companion in a terminal and leave it running:

   ```bash
   cd <repository>/ml
   source .venv/bin/activate
   python -m steadyui_ml.server --token "paste-your-token-here"
   ```

2. In another terminal, confirm it is ready:

   ```bash
   curl http://127.0.0.1:8765/health
   ```

   The expected response is `{"status":"ok"}`.
3. Browse to a normal `http://` or `https://` page, not a `chrome://` page.
4. Press `Alt+Shift+C`, then click each of the five purple calibration targets.
5. Use interactive elements on the page. A press followed by a click on the
   intended element becomes a `success`; clicking elsewhere becomes a `miss`.
   Switching away before completion becomes `abandoned`.
6. Confirm that data was saved:

   ```bash
   cd <repository>/ml
   wc -l data/raw/calibration.jsonl data/raw/trajectories.jsonl
   ```

   The line counts increase as records are collected. If the companion is not
   available, the extension queues up to 100 records locally; reload the
   extension after restarting the companion to retry delivery.

### Train after collecting enough labelled examples

Collect at least two sessions and at least two outcome labels, then run:

```bash
cd <repository>/ml
source .venv/bin/activate
python - <<'PY'
from steadyui_ml import train_model

print(train_model("data/raw/trajectories.jsonl", "models/active-model.joblib"))
PY
```

This creates `models/active-model.joblib` and its metadata file. The current
extension collects data and the companion can serve local predictions, but it
does not yet alter page UI automatically from model predictions.

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
    server.py    # local-only companion HTTP API
    training.py  # local logistic-regression training and model persistence
    prediction.py # local model inference
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

## Local companion demo

### Architecture and data flow

The local-companion setup keeps collection and model data on the same computer
as the browser. The extension has no permission to write directly to this
repository; the Python companion is the process that owns filesystem access.

```text
User moves or clicks in a browser page
  -> extension content script captures bounded pointer samples
  -> extension service worker POSTs JSON to 127.0.0.1
  -> local companion validates and feature-engineers the record
  -> data/raw/*.jsonl (append-only source of truth)
  -> local training job evaluates and saves models/active-model.joblib
  -> local /predict endpoint returns outcome probabilities to the extension
```

The browser can temporarily queue records in `chrome.storage.local`, but that
is only a delivery queue. The source data for training is the JSONL data saved
by the companion in `data/raw/`.

### First-time setup

1. Create and activate the Python environment using the [Set up](#set-up)
   commands above.
2. Generate a private, random token. For example:

   ```bash
   python -c 'import secrets; print(secrets.token_urlsafe(32))'
   ```

3. Start the companion, replacing the example token with the generated value:

   ```bash
   cd ml
   source .venv/bin/activate
   python -m steadyui_ml.server --token "paste-the-generated-token-here"
   ```

   It listens only on `http://127.0.0.1:8765`. Keep this terminal running while
   collecting data or requesting predictions.

4. In Chrome, open `chrome://extensions`, turn on **Developer mode**, choose
   **Load unpacked**, and select the repository's `extension/public` directory.
5. On the SteadyUI extension card, choose **Details** then **Extension options**.
   Enter `http://127.0.0.1:8765` and the same token used to start the companion.
6. Reload the extension after source changes. Open a normal `http://` or
   `https://` page; Chrome does not inject content scripts into its own internal
   pages such as `chrome://extensions`.

The token is stored in Chrome extension storage, not in this repository. Do not
commit it, share it, or put it in source code.

### Collecting browser data

#### Calibration trials

On an ordinary web page, press `Alt+Shift+C`. The extension displays five
purple targets at known viewport coordinates. Click each target. For every
click it saves a separate `CalibrationTrial` with:

- the known target coordinate;
- the pointer coordinate at click time;
- the session ID, monotonic timestamp, and display pixel ratio.

The difference between pointer and target position is the observed calibration
offset. Calibration data is deliberately stored separately from model examples:
it can be used to build a pointer correction profile without being mistaken for
a success/miss outcome label.

#### Labelled trajectories

When a user presses an interactive element, the extension begins an attempt and
keeps a small bounded history of pointer samples. The final click is labelled:

- `success` when it resolves to the same interactive element;
- `miss` when it resolves to a different interactive element or location;
- `abandoned` when the window loses focus before the attempt completes.

The extension sends the target's geometry and stable identifier, samples,
opaque page ID, session ID, and label to `/interactions`. It does not send page
text, form values, or typed input. Prefer setting `data-steadyui-target-id` on
application-owned targets; that gives a stable target ID without relying on a
page-specific fallback.

Open the page's DevTools Console to see collector status. If the companion is
not available, the extension reports that the capture was queued locally.

If the companion is unavailable, the extension holds up to 100 capture records
in `chrome.storage.local` and retries when the extension restarts. Browser
storage is only a queue; JSONL in `data/raw/` is the training source of truth.

### Verifying collection

While the companion is running, these commands show that the browser data was
persisted locally:

```bash
cd ml
wc -l data/raw/calibration.jsonl data/raw/trajectories.jsonl
tail -n 1 data/raw/calibration.jsonl
tail -n 1 data/raw/trajectories.jsonl
```

Each line is a complete JSON object. It is safe to append more records, but do
not hand-edit raw records: the files are the append-only source of truth.

The companion's HTTP contract is useful for manual debugging:

| Method and path | Purpose |
| --- | --- |
| `GET /health` | Confirms that the local companion is running. |
| `POST /calibration-trials` | Validates and stores one calibration trial. |
| `POST /interactions` | Validates, calculates features for, and stores one labelled trajectory. |
| `POST /train` | Trains and writes the active local model. |
| `POST /predict` | Returns local outcome probabilities for `{target, samples}`. |

All `POST` requests require `Content-Type: application/json` and the
`X-SteadyUI-Token` header. The server rejects bodies larger than 1 MB.

### Train the local model

The current trainer is a deterministic-feature, standard-scaled logistic
regression classifier. It is not an LLM. It learns the statistical relationship
between movement features and known `success`, `miss`, or `abandoned` labels.

Before training, collect at least two sessions and at least two outcome classes.
`unknown` examples remain in raw storage but are excluded from supervised
training because they have no answer to learn from. Sessions, rather than
individual samples, are split between train and test data to reduce leakage of
one person's movement patterns into both sets.

After collecting labelled trajectories from at least two sessions and at least
two outcome classes, train locally:

```bash
python - <<'PY'
from steadyui_ml import train_model

result = train_model("data/raw/trajectories.jsonl", "models/active-model.joblib")
print(result)
PY
```

The resulting files are:

```text
ml/models/active-model.joblib
ml/models/active-model.joblib.metadata.json
```

The metadata records feature-column order, schema version, train/test counts,
test accuracy, a classification report, and the random seed. The model artifact
is written atomically so a prediction request does not observe a half-written
file.

To trigger the same operation through the running local companion, send an
authenticated empty JSON body to `POST /train`:

```bash
curl -X POST http://127.0.0.1:8765/train \
  -H 'Content-Type: application/json' \
  -H 'X-SteadyUI-Token: paste-the-generated-token-here' \
  --data '{}'
```

### Local inference

`POST /predict` accepts the same target and ordered browser samples used during
collection, calculates the same deterministic feature vector, loads the active
model, and returns a predicted outcome plus one probability per learned label.

```json
{
  "prediction": "miss",
  "probabilities": {"abandoned": 0.03, "miss": 0.78, "success": 0.19}
}
```

An identical saved model and identical feature vector produce an identical
prediction. The probability is a model estimate based on past labelled examples,
not a guarantee about the user. The extension should use this conservatively—for
example, to offer confirmation or increase a target's effective hit area—not to
silently perform an irreversible action.

The saved model accepts the same `{target, samples}` browser payload through
`POST /predict`. The service worker is ready to call this endpoint; the current
content script only persists labelled captures and does not apply automatic UI
assistance based on a prediction yet.

### Privacy, security, and retention

- The companion binds to loopback (`127.0.0.1`) only; it does not listen on the
  LAN.
- The extension and companion share a per-install token. Keep it private.
- Raw and processed data plus model artifacts are excluded from Git by the root
  `.gitignore`; this prevents accidental commits but is not encryption.
- Ask for informed user consent before collecting behavioural pointer data.
- Keep `page_id` and `target_id` opaque. Do not add page text, form values,
  passwords, or other sensitive content to capture payloads.
- Retain raw data only for as long as it is needed, and provide a local deletion
  path in a production version.

### Troubleshooting

| Symptom | Check |
| --- | --- |
| Extension says a record was queued | Confirm the companion terminal is running, then reload the extension to retry its queue. Check the URL and token in Extension options. |
| `401 invalid companion token` | The token in Extension options differs from `--token`; copy the exact same value into both places. |
| No records appear in `data/raw/` | Test `curl http://127.0.0.1:8765/health`, reload the unpacked extension, and use a normal web page rather than `chrome://` or the Chrome Web Store. |
| Training fails with too few labels or sessions | Collect labelled trajectories across at least two sessions and at least two outcomes. |
| Prediction fails because no model exists | Train first; confirm `models/active-model.joblib` exists. |

### Required fields

- `CursorSample`: `x`, `y`, `time_ms`; `velocity_px_s` is retained when provided, otherwise calculated.
- `Target`: a stable `target_id`, target type, and viewport-relative bounding box. Its center is used for target-distance features.
- `InteractionLabel`: `outcome` (`success`, `miss`, `abandoned`, or `unknown`) and its source. Human labels can add `intent` and `confidence`.
- `MovementFeatures`: generated only from the ordered sample sequence and target; it includes timing, distance, speed, acceleration, straightness, direction changes, and final target distance.

`schema_version` is written with every record. Make a new schema version rather than silently changing meanings or units.
