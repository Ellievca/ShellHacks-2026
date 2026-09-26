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

    if (dt === 0) {
        return 0;
    }

    return distance / dt;
}

// Finds all interactive elements on the page
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
    ];
}

// Gets the center coordinates of an element
function getElementCenter(element) {
    const rect = element.getBoundingClientRect();

    return {
        x: rect.left + rect.width / 2,
        y: rect.top + rect.height / 2
    }
}

// Distance 
function calculateDistance(pointA, pointB) {
    const dx = pointA.x - pointB.x;
    const dy = pointA.y - pointB.y;

    return Math.sqrt(dx * dx + dy * dy);
}

// Finds and ranks nearest interactive elements
function findNearestElements(point, limit = 5) {
    const elements = findInteractiveElements();

    const ranked = elements.map(element => {
        const center = getElementCenter(element);
        const distance = calculateDistance(point, center);

        return {
            element,
            center,
            distance
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

    // Connect point to nearest interactive elements
    const nearestElements = findNearestElements(point, 5);

    console.log(
    nearestElements.map((target) => ({
        element: target.element.innerText || target.element.getAttribute("aria-label") || target.element.tagName,
        distance: target.distance
    }))
);

    // Gets previous point
    const previous = movements[movements.length - 1];

    // Calculates velocity if a previous point exists
    if (previous) {
        point.velocity = calculateVelocity(previous, point);
    }

    movements.push(point);

    // Stores the most recent 100 movements
    if (movements.length > 100) {
        movements.shift();
    }

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