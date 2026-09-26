console.log("SteadyUI initialized!");

// Stores mouse movements and timestamps 
const movements = [];

// Tracks when last mouse movement was recorded
let lastRecordedTime = 0;

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

const elements = findInteractiveElements();

// debug
elements.forEach((element) => {
    element.style.outline = "2px solid purple";
});

console.log(
    `SteadyUI found ${elements.length} interactive elements`
);