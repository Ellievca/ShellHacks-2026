console.log("SteadyUI initialized!");

// Stores mouse movements and timestamps 
const movements = [];

// Tracks when last mouse movement was recorded
let lastRecordedTime = 0;

// Padding the intent expansion tiers are tuned for; the native profile scales relative to this
const BASELINE_HITBOX_PADDING = 20;

// Set from the zeroTremor native host's tremor profile once it loads
let nativeHitboxPadding = BASELINE_HITBOX_PADDING;

// Intent scoring settings
const INTENT_DISTANCE_LIMIT = 180;
const TRAJECTORY_WINDOW = 6;

// Intent assistance settings
const MIN_INTENT_SCORE = 0.55;
const MIN_INTENT_MARGIN = 0.1; // avoid ambiguity between 2 close elements

// Velocity calculation function
function calculateVelocity(previous, current) {
    // Calculate distance and time diff
    const dx = current.x - previous.x;
    const dy = current.y - previous.y;
    const dt = (current.time - previous.time)/ 1000;

    const distance = Math.sqrt(dx * dx + dy * dy);

    if (dt === 0) return 0;

    return distance / dt;
}

//-----------------------------------------
// Interactive Element Detection + Ranking
//-----------------------------------------

// Finds all visible interactive elements on the page
function findInteractiveElements() {
    const selectors = [
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

    return [
        ...document.querySelectorAll(selectors.join(","))
    ].filter((element) => {
        const rect = element.getBoundingClientRect();

        return rect.width > 0 && rect.height > 0
    });
}

function describeTargetElement(element) {
    const rect = element.getBoundingClientRect();

    return {
        tag: element.tagName,
        text: element.innerText?.trim() || "",
        ariaLabel: element.getAttribute("aria-label"),
        width: rect.width,
        height: rect.height,
    };
}

// Calculates distance from mouse to the closest element
function calculateDistanceToElement(point, element) {
    const rect = element.getBoundingClientRect();

    const closestX = Math.max(rect.left, Math.min(point.x, rect.right));
    const closestY = Math.max(rect.top, Math.min(point.y, rect.bottom));

    const dx = point.x - closestX;
    const dy = point.y - closestY;

    return Math.sqrt(dx * dx + dy * dy);
}

// Finds and ranks nearest interactive elements
function findNearestElements(point, limit = 5) {
    const elements = findInteractiveElements();

    const ranked = elements.map(element => {
        const distance = calculateDistanceToElement(point, element);

        return {
            element,
            distance,
            metadata: describeTargetElement(element)
        };
    });

    // Sorts by distance
    ranked.sort((a, b) => a.distance - b.distance);

    return ranked.slice(0, limit);
}

function findExpandedHitTarget(point) {
    const rankedTargets = rankTargetsByIntent(point, 5);

    if (rankedTargets.length === 0) return null;

    const bestTarget = rankedTargets[0];
    const secondTarget = rankedTargets[1];

    // Low confidence score
    if (bestTarget.intentScore < MIN_INTENT_SCORE) return null;

    // Ambiguous between 2 targets
    if (secondTarget && (bestTarget.intentScore - secondTarget.intentScore) < MIN_INTENT_MARGIN) return null;

    const expansion = getExpansionByIntent(bestTarget.intentScore);

    const distance = calculateDistanceToElement(point, bestTarget.element);

    // Clicked within adaptive target area
    if (distance <= expansion) {
        return {
            ...bestTarget,
            expansion
        };
    }
}

// Determine whether the user has already clicked an element
function getInteractiveAncestor(element) {
    return element.closest(
        [
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
        ].join(",")
    );
}

//---------------------------
// Mouse Movement Event Listener
//---------------------------

// Listens for mouse movements
document.addEventListener("mousemove", (event) => {
    const now = performance.now();

    // Records roughly 20 samples/sec
    if (now - lastRecordedTime < 50) {
        return;
    }

    lastRecordedTime = now;

    // Coordinates
    const point = {
        x: event.clientX,
        y: event.clientY,
        time: now
    };

    // Gets previous point
    const previous = movements[movements.length - 1];

    // Calculates velocity if a previous point exists
    if (previous) {
        point.velocity = calculateVelocity(previous, point);
    }

    // Store current point
    movements.push(point);

    // Stores the most recent 100 movements
    if (movements.length > 100) {
        movements.shift();
    }

    // Connect point to 5 nearest interactive elements
    const nearestElements = findNearestElements(point, 5);

    // Rank the nearest elements by intent score
    const intentTargets = rankTargetsByIntent(point, 5);

    updateAdaptiveHitboxes(point);
    
    // Debug: Console output
    console.log(
        "Intent ranking:",
        intentTargets.map((target) => ({
            target:
                target.metadata.text ||
                target.metadata.ariaLabel ||
                target.metadata.tag,

            intent:
                Math.round(target.intentScore * 100) + "%",

            distance:
                Math.round(target.distance),

            direction:
                Math.round(target.scores.direction * 100) + "%",

            approach:
                Math.round(target.scores.approach * 100) + "%"
        }))
    );
});

//---------------------------
// Click Event Listener
//---------------------------
// Detects whether the user is actually clicking on an element or just moving the mouse over it
let isSteadyUIClick = false;

// Prevent synthetic click from getting intercepted twice
document.addEventListener("click", (event) => {
    if(isSteadyUIClick) return;
    
    // If user clicks normally, don't intercept
    const realTarget = getInteractiveAncestor(event.target);

    if(realTarget) return;

    const point = {
        x: event.clientX,
        y: event.clientY,
    };

    const expandedTarget = findExpandedHitTarget(point);

    if(!expandedTarget) return;

    console.log("SteadyUI assistance activated: ", 
    {
        target:
            expandedTarget.metadata.text ||
            expandedTarget.metadata.ariaLabel ||
            expandedTarget.metadata.tag,

        intent: 
            Math.round(expandedTarget.intentScore * 100) + "%",

        expansion:
            expandedTarget.expansion + "px",

        distance:
            Math.round(expandedTarget.distance) + "px"
    });

    // Prevent misclick
    event.preventDefault();
    event.stopPropagation();

    const element = expandedTarget.element;

    isSteadyUIClick = true;
    
    // Check for special-case text fields
    if (element.matches("input, textarea, select")) {
        element.focus();
    } else {
        element.click();
    }

    isSteadyUIClick = false;
},
// Use the capture phase so SteadyUI examines the click before its handled by webpage
true
);

const elements = findInteractiveElements();

// ---------------------------
// DEBUG VISUALIZATION
// ---------------------------

// Purple = actual clickable element
// Red dashed = default 20px expanded hitbox

function updateAdaptiveHitboxes(point) {
    // Remove previous debug boxes
    document
        .querySelectorAll(".debug-hitbox-overlay")
        .forEach((overlay) => overlay.remove());

    const elements = findInteractiveElements();

    elements.forEach((element) => {

        // Build the same target object used by intent scoring
        const target = {
            element: element,
            distance: calculateDistanceToElement(point, element),
            metadata: describeTargetElement(element)
        };

        // Calculate THIS element's current intent score
        const scoredTarget =
            calculateIntentScore(point, target);

        // Convert intent → adaptive expansion
        const expansion =
            getExpansionByIntent(
                scoredTarget.intentScore
            );

        const rect =
            element.getBoundingClientRect();

        // Purple = actual HTML element
        element.style.outline =
            "2px solid purple";

        // Red = SteadyUI's CURRENT adaptive area
        const overlay =
            document.createElement("div");

        overlay.style.position = "fixed";

        overlay.style.left =
            `${rect.left - expansion}px`;

        overlay.style.top =
            `${rect.top - expansion}px`;

        overlay.style.width =
            `${rect.width + expansion * 2}px`;

        overlay.style.height =
            `${rect.height + expansion * 2}px`;

        overlay.style.border =
            "2px dashed red";

        overlay.style.pointerEvents =
            "none";

        overlay.style.zIndex =
            "999999";

        overlay.className =
            "debug-hitbox-overlay";

        document.body.appendChild(overlay);
    });
}

// -------------------------
// Trajectory Intent Scoring
// -------------------------

function clamp01(value) {
    return Math.max(0, Math.min(1, value)); // Keeps scores between 0 and 1
}

// Get center of element to calculate direction of travel
function getElementCenter(element) {
    const rect = element.getBoundingClientRect();

    return {
        x: rect.left + rect.width / 2,
        y: rect.top + rect.height / 2
    };
}

// Compares where the mouse is moving to the element's location
function calculateDistanceScore(point, element) {
    const distance = calculateDistanceToElement(point, element);

    return clamp01(1 - distance / INTENT_DISTANCE_LIMIT);
}

// Use cosine similarity to calculate direction score
function calculateDirectionScore(point, element) {
    if(movements.length < 2) return 0.5;

    const startIndex = Math.max(0, movements.length - TRAJECTORY_WINDOW);

    const startPoint = movements[startIndex];

    // Cursor movement vector
    const movementX = point.x - startPoint.x;
    const movementY = point.y - startPoint.y;

    // Target direction vector
    const center = getElementCenter(element);
    const targetX = center.x - point.x;
    const targetY = center.y - point.y;

    // Magnitudes
    const movementMagnitude = Math.sqrt(movementX * movementX + movementY * movementY);
    const targetMagnitude = Math.sqrt(targetX * targetX + targetY * targetY);

    // Edge case: cursor movement not significant
    if(movementMagnitude < 1) return 0.5;

    // Cursor roughly at target center
    if(targetMagnitude < 1) return 1;

    const dotProduct = movementX * targetX + movementY * targetY;

    const cosine = dotProduct / (movementMagnitude * targetMagnitude);

    return clamp01((cosine + 1) / 2);
}

// Find out if cursor has been approaching element
function calculateApproachScore(point, element) {
    if (movements.length < 2) return 0.5;

    const startIndex = Math.max(0, movements.length - TRAJECTORY_WINDOW);

    const startPoint = movements[startIndex];

    const previousDistance = calculateDistanceToElement(startPoint, element);
    const currentDistance = calculateDistanceToElement(point, element);

    // Diff: how much closer did we get
    const diff = previousDistance - currentDistance;

    return clamp01(diff / 40); // 40px diff counts as a strong approach score
}

// Take the weighted average of distance, direction, and approach scores to determine intent score
function calculateIntentScore(point, target) {
    const distanceScore = calculateDistanceScore(point, target.element);
    const directionScore = calculateDirectionScore(point, target.element);
    const approachScore = calculateApproachScore(point, target.element);

    // Weighted average: 40% distance, 40% movement direction, 20% approach trend
    const intentScore = 
    (
        distanceScore * 0.4 +
        directionScore * 0.4 +
        approachScore * 0.2
    );

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

//---------------------------
// Ranking by score
//---------------------------

function rankTargetsByIntent(point, limit = 5) {
    const candidates = findNearestElements(point, 10);

    const scored = candidates.map(target => calculateIntentScore(point, target));

    scored.sort((a, b) => b.intentScore - a.intentScore);

    return scored.slice(0, limit);
}

// Makes hitbox size depend on intent score, scaled by the user's tremor profile.
function getExpansionByIntent(intentScore) {
    const scale = nativeHitboxPadding / BASELINE_HITBOX_PADDING;

    if (intentScore >= 0.85) return 35 * scale;

    if (intentScore >= 0.7) return 25 * scale;

    if (intentScore >= 0.55) return 15 * scale;

    return 0;
}

//---------------------------
// Native Tremor Profile
//---------------------------

function paddingFromTremorAmplitude(amplitude) {
    if (amplitude < 3) return 6;

    if (amplitude < 6) return 12;

    if (amplitude < 10) return 20;

    return 28;
}

function loadZeroTremorProfile() {
    chrome.runtime.sendMessage(
        { type: "ZEROTREMOR_GET_PROFILE" },
        (result) => {
            if (chrome.runtime.lastError) {
                console.error("[zeroTremor] Profile request failed:", chrome.runtime.lastError.message);
                return;
            }

            if (!result?.ok) {
                console.error("[zeroTremor] Native engine error:", result?.error);
                return;
            }

            const profile = result.response;

            if (typeof profile?.tremorAmplitude !== "number") {
                console.error("[zeroTremor] Unexpected profile response:", profile);
                return;
            }

            nativeHitboxPadding = paddingFromTremorAmplitude(profile.tremorAmplitude);

            console.log("[zeroTremor] Native profile loaded:", profile);
            console.log("[zeroTremor] Adaptive hitbox padding:", `${nativeHitboxPadding}px`);
        }
    );
}

loadZeroTremorProfile();