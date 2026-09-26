console.log("SteadyUI initialized!");

// Stores mouse movements and timestamps 
const movements = [];

// Tracks when last mouse movement was recorded
let lastRecordedTime = 0;

// 20px outside an element will still count as a hit (debug), this will be personalized later
const HITBOX_EXPANSION = 20;

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
    const nearest = findNearestElements(point, 1);

    if (nearest.length === 0) return null;

    const target = nearest[0];

    if (target.distance <= HITBOX_EXPANSION) return target;

    return null;
}

// Determine whether the user has already clicked an element
function getPreviousElement(element) {
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

    // debug cursor loc
    console.log("Cursor:", point);

    // debug nearby elements
    console.log("Nearest interactive elements:", nearestElements.map(e => e.metadata));

    console.log(point);
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
    const realTarget = getPreviousElement(event.target);

    if(realTarget) return;

    const point = {
        x: event.clientX,
        y: event.clientY,
    };

    const expandedTarget = findExpandedHitTarget(point);

    if(!expandedTarget) return;

    console.log("Expanded hitbox activated: ", expandedTarget.metadata);

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

// DEBUG: 
// Purple - outlines interactive elements
// Red - outlines expanded hitboxes

function showExpandedHitboxes() {
    const elements = findInteractiveElements();

    elements.forEach((element) => {
        const rect = element.getBoundingClientRect();

        const overlay = document.createElement("div");

        overlay.style.position = "fixed";

        overlay.style.left = `${rect.left - HITBOX_EXPANSION}px`;
        overlay.style.top = `${rect.top - HITBOX_EXPANSION}px`;
        overlay.style.width = `${rect.width + HITBOX_EXPANSION * 2}px`;
        overlay.style.height = `${rect.height + HITBOX_EXPANSION * 2}px`;
        overlay.style.border = "1px dashed red";
        overlay.style.pointerEvents = "none";
        overlay.style.zIndex = "999999";
        overlay.className = "debug-hitbox-overlay";
        
        document.body.appendChild(overlay);
    });
}

showExpandedHitboxes();

elements.forEach((element) => {
    element.style.outline = "2px solid purple";
});

console.log(
    `SteadyUI found ${elements.length} interactive elements`
);