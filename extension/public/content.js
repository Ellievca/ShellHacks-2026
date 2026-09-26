console.info("SteadyUI collector initialized. Press Alt+Shift+C to calibrate.");

const SAMPLE_INTERVAL_MS = 50;
const MAX_SAMPLES = 100;
const sessionId = crypto.randomUUID();
const pageId = `page-${[...`${location.origin}${location.pathname}`].reduce(
  (hash, character) => ((hash * 31 + character.charCodeAt(0)) >>> 0), 0
).toString(16)}`;
const interactiveSelector = [
  "button", "a[href]", "input", "select", "textarea", "summary",
  "[role='button']", "[role='link']", "[role='checkbox']", "[role='radio']",
  "[role='switch']", "[role='slider']",
].join(",");

let lastSampleTime = 0;
let recentSamples = [];
let activeAttempt = null;
let calibration = null;
let calibrationIndex = 0;

function browserSample(event) {
  const time = performance.now();
  const previous = recentSamples.at(-1);
  const distance = previous ? Math.hypot(event.clientX - previous.x, event.clientY - previous.y) : 0;
  const seconds = previous ? (time - previous.time) / 1000 : 0;
  return { x: event.clientX, y: event.clientY, time, ...(seconds > 0 ? { velocity: distance / seconds } : {}) };
}

function recordSample(event, force = false) {
  const time = performance.now();
  if (!force && time - lastSampleTime < SAMPLE_INTERVAL_MS) return null;
  lastSampleTime = time;
  const sample = browserSample(event);
  recentSamples = [...recentSamples, sample].slice(-MAX_SAMPLES);
  if (activeAttempt) activeAttempt.samples.push(sample);
  return sample;
}

function interactiveElement(node) {
  return node instanceof Element ? node.closest(interactiveSelector) : null;
}

function targetFor(element) {
  const bounds = element.getBoundingClientRect();
  const index = [...document.querySelectorAll(interactiveSelector)].indexOf(element);
  return {
    // Prefer an explicit app-owned ID. The fallback deliberately contains no page text.
    target_id: element.dataset.steadyuiTargetId || element.id || `${element.tagName.toLowerCase()}-${index}`,
    target_type: element.getAttribute("role") || element.tagName.toLowerCase(),
    bounds: { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height },
  };
}

function submit(type, payload) {
  chrome.runtime.sendMessage({ type, payload }, (response) => {
    if (chrome.runtime.lastError) console.warn("SteadyUI could not reach its service worker.");
    else if (!response?.ok) console.warn(response.queued ? "SteadyUI queued this capture locally." : response.error);
  });
}

document.addEventListener("pointermove", (event) => recordSample(event));

document.addEventListener("pointerdown", (event) => {
  if (calibration?.target === event.target) return;
  const element = interactiveElement(event.target);
  if (!element) return;
  recordSample(event, true);
  activeAttempt = {
    interactionId: crypto.randomUUID(), targetElement: element, target: targetFor(element),
    samples: recentSamples.slice(-10),
  };
}, true);

document.addEventListener("click", (event) => {
  if (calibration?.target === event.target) {
    const trial = calibration;
    calibration = null;
    event.preventDefault();
    event.stopPropagation();
    trial.target.remove();
    submit("calibration", {
      trial_id: crypto.randomUUID(), session_id: sessionId, target_x: trial.x, target_y: trial.y,
      pointer_x: event.clientX, pointer_y: event.clientY, time_ms: performance.now(),
      device_pixel_ratio: window.devicePixelRatio,
    });
    window.setTimeout(showNextCalibrationTarget, 250);
    return;
  }
  if (!activeAttempt) return;
  const attempt = activeAttempt;
  activeAttempt = null;
  const completedTarget = interactiveElement(event.target);
  const sample = recordSample(event, true);
  if (sample && attempt.samples.at(-1) !== sample) attempt.samples.push(sample);
  submit("interaction", {
    interaction_id: attempt.interactionId, session_id: sessionId,
    page_id: pageId,
    target: attempt.target, samples: attempt.samples,
    label: { outcome: completedTarget === attempt.targetElement ? "success" : "miss", source: "instrumentation" },
  });
}, true);

window.addEventListener("blur", () => {
  if (!activeAttempt) return;
  const attempt = activeAttempt;
  activeAttempt = null;
  submit("interaction", {
    interaction_id: attempt.interactionId, session_id: sessionId,
    page_id: pageId, target: attempt.target, samples: attempt.samples,
    label: { outcome: "abandoned", source: "instrumentation" },
  });
});

const calibrationPositions = [[.5, .5], [.15, .2], [.85, .2], [.15, .8], [.85, .8]];

function showNextCalibrationTarget() {
  if (calibrationIndex >= calibrationPositions.length) {
    calibrationIndex = 0;
    console.info("SteadyUI calibration complete.");
    return;
  }
  const [horizontal, vertical] = calibrationPositions[calibrationIndex++];
  const x = Math.round(innerWidth * horizontal);
  const y = Math.round(innerHeight * vertical);
  const target = document.createElement("button");
  target.type = "button";
  target.setAttribute("aria-label", "Calibration target");
  target.style.cssText = `background:#6d28d9;border:3px solid white;border-radius:50%;box-shadow:0 0 0 2px #6d28d9;cursor:crosshair;height:28px;left:${x - 14}px;position:fixed;top:${y - 14}px;width:28px;z-index:2147483647;`;
  document.documentElement.append(target);
  calibration = { target, x, y };
}

window.addEventListener("keydown", (event) => {
  if (event.altKey && event.shiftKey && event.key.toLowerCase() === "c" && !calibration) {
    event.preventDefault();
    showNextCalibrationTarget();
  }
});
