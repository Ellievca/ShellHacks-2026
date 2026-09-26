console.log("SteadyUI initialized!");

// Velocity calculation function
function calculateVelocity(previous, current) {
    // Calculate distance and time diff
    const dx = current.x - previous.x;
    const dy = current.y - previous.y;
    const dt = (current.time - previous.time)/ 1000;

    const distance = Math.sqrt(
        dx * dx + dy * dy
    );

    if (dt === 0) {
        return 0;
    }

    // returns velocity
    return distance / dt;
}

// Stores mouse movements and timestamps 
const movements = [];

// Limits number of mouse movements recorded
let lastRecordedTime = 0;

// Listens for mouse movements and log coordinates and timestamps
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
        const velocity = calculateVelocity(previous, point);
        point.velocity = velocity;
    }

    movements.push(point);

    // Stores the most recent 100 movements
    if (movements.length > 100) {
        movements.shift();
    }

    console.log(point);
});