console.log("SteadyUI initialized!");

// Stores mouse movements and timestamps
const movements = [];

// Tracks when last mouse movement was recorded
let lastRecordedTime = 0;

// Baseline padding the intent-expansion tiers are tuned for
const BASELINE_HITBOX_PADDING = 20;

// Updated from standalone calibration profile
let adaptiveHitboxPadding = BASELINE_HITBOX_PADDING;

// Intent scoring settings
const INTENT_DISTANCE_LIMIT = 180;
const TRAJECTORY_WINDOW = 6;

// Intent assistance settings
const MIN_INTENT_SCORE = 0.55;
const MIN_INTENT_MARGIN = 0.1;

// Debug visualization
const DEBUG_VISUALS = true;

// ---------------------------
// Helpers
// ---------------------------

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function clamp01(value) {
  return clamp(value, 0, 1);
}

function calculateVelocity(previous, current) {
  const dx = current.x - previous.x;
  const dy = current.y - previous.y;
  const dt = (current.time - previous.time) / 1000;

  if (dt <= 0) return 0;

  const distance = Math.hypot(dx, dy);

  return distance / dt;
}

// -----------------------------------------
// Interactive Element Detection + Ranking
// -----------------------------------------

const INTERACTIVE_SELECTORS = [
  "button",
  "a[href]",
  "input",
  "select",
  "textarea",
  "summary",
  "[role='button']",
  "[role='link']",
  "[role='checkbox']",
  "[role='radio']",
  "[role='switch']",
  "[role='slider']"
];

function findInteractiveElements() {
  return [
    ...document.querySelectorAll(
      INTERACTIVE_SELECTORS.join(",")
    )
  ].filter((element) => {
    const rect = element.getBoundingClientRect();

    if (rect.width <= 0 || rect.height <= 0) {
      return false;
    }

    const style = window.getComputedStyle(element);

    if (
      style.visibility === "hidden" ||
      style.display === "none" ||
      Number(style.opacity) === 0
    ) {
      return false;
    }

    if (element.matches(":disabled")) {
      return false;
    }

    return true;
  });
}

function describeTargetElement(element) {
  const rect = element.getBoundingClientRect();

  return {
    tag: element.tagName,
    text:
      element.innerText?.trim().slice(0, 80) ||
      element.value?.toString().slice(0, 80) ||
      "",
    ariaLabel: element.getAttribute("aria-label"),
    width: rect.width,
    height: rect.height
  };
}

function calculateDistanceToElement(point, element) {
  const rect = element.getBoundingClientRect();

  const closestX = Math.max(
    rect.left,
    Math.min(point.x, rect.right)
  );

  const closestY = Math.max(
    rect.top,
    Math.min(point.y, rect.bottom)
  );

  const dx = point.x - closestX;
  const dy = point.y - closestY;

  return Math.hypot(dx, dy);
}

function findNearestElements(point, limit = 5) {
  const elements = findInteractiveElements();

  const ranked = elements.map((element) => {
    const distance =
      calculateDistanceToElement(point, element);

    return {
      element,
      distance,
      metadata: describeTargetElement(element)
    };
  });

  ranked.sort(
    (a, b) => a.distance - b.distance
  );

  return ranked.slice(0, limit);
}

function getInteractiveAncestor(element) {
  if (!(element instanceof Element)) {
    return null;
  }

  return element.closest(
    INTERACTIVE_SELECTORS.join(",")
  );
}

// -------------------------
// Trajectory Intent Scoring
// -------------------------

function getElementCenter(element) {
  const rect = element.getBoundingClientRect();

  return {
    x: rect.left + rect.width / 2,
    y: rect.top + rect.height / 2
  };
}

function calculateDistanceScore(point, element) {
  const distance =
    calculateDistanceToElement(point, element);

  return clamp01(
    1 - distance / INTENT_DISTANCE_LIMIT
  );
}

function calculateDirectionScore(point, element) {
  if (movements.length < 2) {
    return 0.5;
  }

  const startIndex = Math.max(
    0,
    movements.length - TRAJECTORY_WINDOW
  );

  const startPoint = movements[startIndex];

  const movementX =
    point.x - startPoint.x;

  const movementY =
    point.y - startPoint.y;

  const center =
    getElementCenter(element);

  const targetX =
    center.x - point.x;

  const targetY =
    center.y - point.y;

  const movementMagnitude =
    Math.hypot(movementX, movementY);

  const targetMagnitude =
    Math.hypot(targetX, targetY);

  if (movementMagnitude < 1) {
    return 0.5;
  }

  if (targetMagnitude < 1) {
    return 1;
  }

  const dotProduct =
    movementX * targetX +
    movementY * targetY;

  const cosine =
    dotProduct /
    (movementMagnitude * targetMagnitude);

  return clamp01(
    (cosine + 1) / 2
  );
}

function calculateApproachScore(point, element) {
  if (movements.length < 2) {
    return 0.5;
  }

  const startIndex = Math.max(
    0,
    movements.length - TRAJECTORY_WINDOW
  );

  const startPoint =
    movements[startIndex];

  const previousDistance =
    calculateDistanceToElement(
      startPoint,
      element
    );

  const currentDistance =
    calculateDistanceToElement(
      point,
      element
    );

  const diff =
    previousDistance - currentDistance;

  return clamp01(
    diff / 40
  );
}

function calculateIntentScore(point, target) {
  const distanceScore =
    calculateDistanceScore(
      point,
      target.element
    );

  const directionScore =
    calculateDirectionScore(
      point,
      target.element
    );

  const approachScore =
    calculateApproachScore(
      point,
      target.element
    );

  const intentScore =
    distanceScore * 0.4 +
    directionScore * 0.4 +
    approachScore * 0.2;

  return {
    ...target,
    intentScore,
    scores: {
      distance: distanceScore,
      direction: directionScore,
      approach: approachScore
    }
  };
}

function rankTargetsByIntent(
  point,
  limit = 5
) {
  const candidates =
    findNearestElements(point, 10);

  const scored =
    candidates.map((target) =>
      calculateIntentScore(
        point,
        target
      )
    );

  scored.sort(
    (a, b) =>
      b.intentScore -
      a.intentScore
  );

  return scored.slice(0, limit);
}

// ---------------------------------
// Adaptive Personalized Hitbox Size
// ---------------------------------

function getExpansionByIntent(
  intentScore
) {
  const scale =
    adaptiveHitboxPadding /
    BASELINE_HITBOX_PADDING;

  if (intentScore >= 0.85) {
    return 35 * scale;
  }

  if (intentScore >= 0.7) {
    return 25 * scale;
  }

  if (intentScore >= 0.55) {
    return 15 * scale;
  }

  return 0;
}

function findExpandedHitTarget(point) {
  const rankedTargets =
    rankTargetsByIntent(point, 5);

  if (
    rankedTargets.length === 0
  ) {
    return null;
  }

  const bestTarget =
    rankedTargets[0];

  const secondTarget =
    rankedTargets[1];

  if (
    bestTarget.intentScore <
    MIN_INTENT_SCORE
  ) {
    return null;
  }

  if (
    secondTarget &&
    bestTarget.intentScore -
      secondTarget.intentScore <
      MIN_INTENT_MARGIN
  ) {
    return null;
  }

  const expansion =
    getExpansionByIntent(
      bestTarget.intentScore
    );

  const distance =
    calculateDistanceToElement(
      point,
      bestTarget.element
    );

  if (distance <= expansion) {
    return {
      ...bestTarget,
      expansion
    };
  }

  return null;
}

// ---------------------------
// Mouse Movement Listener
// ---------------------------

document.addEventListener(
  "mousemove",
  (event) => {
    const now =
      performance.now();

    // ~20 samples/sec
    if (
      now -
        lastRecordedTime <
      50
    ) {
      return;
    }

    lastRecordedTime = now;

    const point = {
      x: event.clientX,
      y: event.clientY,
      time: now
    };

    const previous =
      movements[
        movements.length - 1
      ];

    if (previous) {
      point.velocity =
        calculateVelocity(
          previous,
          point
        );
    }

    movements.push(point);

    if (
      movements.length >
      100
    ) {
      movements.shift();
    }

    const intentTargets =
      rankTargetsByIntent(
        point,
        5
      );

    if (DEBUG_VISUALS) {
      updateAdaptiveHitboxes(
        point,
        intentTargets
      );
    }
  }
);

// ---------------------------
// Assisted Click Handling
// ---------------------------

let isSteadyUIClick = false;

document.addEventListener(
  "click",
  (event) => {
    if (isSteadyUIClick) {
      return;
    }

    // User already clicked a real interactive element
    const realTarget =
      getInteractiveAncestor(
        event.target
      );

    if (realTarget) {
      return;
    }

    const point = {
      x: event.clientX,
      y: event.clientY
    };

    const expandedTarget =
      findExpandedHitTarget(
        point
      );

    if (!expandedTarget) {
      return;
    }

    console.log(
      "[SteadyUI] assistance activated",
      {
        target:
          expandedTarget.metadata
            .text ||
          expandedTarget.metadata
            .ariaLabel ||
          expandedTarget.metadata
            .tag,

        intent:
          Math.round(
            expandedTarget
              .intentScore *
              100
          ) + "%",

        expansion:
          Math.round(
            expandedTarget.expansion
          ) + "px",

        distance:
          Math.round(
            expandedTarget.distance
          ) + "px"
      }
    );

    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();

    activateAssistedTarget(
      expandedTarget.element
    );
  },
  true
);

function activateAssistedTarget(
  element
) {
  if (!element) {
    return;
  }

  isSteadyUIClick = true;

  try {
    // Text fields should receive focus
    if (
      element.matches(
        "input:not([type='button']):not([type='submit']):not([type='reset']):not([type='checkbox']):not([type='radio']), textarea"
      )
    ) {
      element.focus();
      return;
    }

    // Select boxes should focus/open normally
    if (
      element.matches(
        "select"
      )
    ) {
      element.focus();
      element.click();
      return;
    }

    // Buttons, links, checkbox, radio, etc.
    element.click();
  } finally {
    window.setTimeout(() => {
      isSteadyUIClick = false;
    }, 0);
  }
}

// ---------------------------
// Debug Visualization
// ---------------------------

const outlinedElements =
  new Set();

function clearDebugVisuals() {
  document
    .querySelectorAll(
      ".steadyui-debug-hitbox"
    )
    .forEach((overlay) =>
      overlay.remove()
    );

  outlinedElements.forEach(
    (element) => {
      if (
        element &&
        element.style
      ) {
        element.style.removeProperty(
          "outline"
        );

        element.style.removeProperty(
          "outline-offset"
        );
      }
    }
  );

  outlinedElements.clear();
}

function updateAdaptiveHitboxes(
  point,
  rankedTargets = null
) {
  clearDebugVisuals();

  const targets =
    rankedTargets ||
    rankTargetsByIntent(
      point,
      5
    );

  targets.forEach(
    (scoredTarget) => {
      if (
        !scoredTarget ||
        !scoredTarget.element
      ) {
        return;
      }

      const {
        element,
        intentScore,
        distance
      } = scoredTarget;

      // Keep visualization local to relevant nearby candidates
      if (
        distance >
        INTENT_DISTANCE_LIMIT
      ) {
        return;
      }

      const expansion =
        getExpansionByIntent(
          intentScore
        );

      if (
        expansion <= 0
      ) {
        return;
      }

      const rect =
        element.getBoundingClientRect();

      // Actual element
      element.style.outline =
        "2px solid #8b5cf6";

      element.style.outlineOffset =
        "1px";

      outlinedElements.add(
        element
      );

      // Adaptive area
      const overlay =
        document.createElement(
          "div"
        );

      overlay.className =
        "steadyui-debug-hitbox";

      overlay.style.position =
        "fixed";

      overlay.style.left =
        `${
          rect.left -
          expansion
        }px`;

      overlay.style.top =
        `${
          rect.top -
          expansion
        }px`;

      overlay.style.width =
        `${
          rect.width +
          expansion * 2
        }px`;

      overlay.style.height =
        `${
          rect.height +
          expansion * 2
        }px`;

      overlay.style.border =
        intentScore >=
        MIN_INTENT_SCORE
          ? "2px dashed red"
          : "1px dashed rgba(255, 0, 0, 0.35)";

      overlay.style.borderRadius =
        "6px";

      overlay.style.pointerEvents =
        "none";

      overlay.style.zIndex =
        "2147483647";

      overlay.style.boxSizing =
        "border-box";

      overlay.style.opacity =
        String(
          clamp(
            0.3 +
              intentScore *
                0.7,
            0.3,
            1
          )
        );

      document.body.appendChild(
        overlay
      );
    }
  );
}

// Clean visuals when page loses focus
window.addEventListener(
  "blur",
  clearDebugVisuals
);

// ---------------------------------
// Standalone SteadyUI Profile
// ---------------------------------

function applySteadyUIProfile(
  profile
) {
  if (
    !profile ||
    typeof profile !==
      "object"
  ) {
    adaptiveHitboxPadding =
      BASELINE_HITBOX_PADDING;

    return;
  }

  const rawPadding =
    Number(
      profile.hitboxPadding
    );

  if (
    Number.isFinite(
      rawPadding
    )
  ) {
    adaptiveHitboxPadding =
      clamp(
        rawPadding,
        6,
        50
      );
  } else {
    adaptiveHitboxPadding =
      BASELINE_HITBOX_PADDING;
  }

  console.log(
    "[SteadyUI] personalized profile loaded",
    profile
  );

  console.log(
    "[SteadyUI] adaptive hitbox padding:",
    `${adaptiveHitboxPadding}px`
  );
}

function loadSteadyUIProfile() {
  if (
    typeof chrome ===
      "undefined" ||
    !chrome.storage ||
    !chrome.storage.local
  ) {
    console.warn(
      "[SteadyUI] Chrome storage unavailable. Using default profile."
    );

    adaptiveHitboxPadding =
      BASELINE_HITBOX_PADDING;

    return;
  }

  chrome.storage.local.get(
    ["steadyUIProfile"],
    (result) => {
      if (
        chrome.runtime
          ?.lastError
      ) {
        console.warn(
          "[SteadyUI] Could not load profile:",
          chrome.runtime
            .lastError.message
        );

        adaptiveHitboxPadding =
          BASELINE_HITBOX_PADDING;

        return;
      }

      const profile =
        result.steadyUIProfile;

      if (!profile) {
        adaptiveHitboxPadding =
          BASELINE_HITBOX_PADDING;

        console.log(
          "[SteadyUI] No calibration profile yet. Using default:",
          `${adaptiveHitboxPadding}px`
        );

        return;
      }

      applySteadyUIProfile(
        profile
      );
    }
  );
}

if (
  typeof chrome !==
    "undefined" &&
  chrome.storage?.onChanged
) {
  chrome.storage.onChanged.addListener(
    (
      changes,
      areaName
    ) => {
      if (
        areaName !==
        "local"
      ) {
        return;
      }

      const change =
        changes
          .steadyUIProfile;

      if (!change) {
        return;
      }

      if (
        change.newValue
      ) {
        applySteadyUIProfile(
          change.newValue
        );
      } else {
        adaptiveHitboxPadding =
          BASELINE_HITBOX_PADDING;

        console.log(
          "[SteadyUI] calibration removed; reverted to default profile"
        );
      }
    }
  );
}

loadSteadyUIProfile();