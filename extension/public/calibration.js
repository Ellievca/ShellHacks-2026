const TOTAL_TRIALS = 20;

const targetSizes = [
  64,
  56,
  48,
  40,
  32
];


const testArea =
  document.getElementById("testArea");

const target =
  document.getElementById("target");

const startButton =
  document.getElementById("startButton");

const downloadRawButton =
  document.getElementById(
    "downloadRawButton"
  );

const downloadFeaturesButton =
  document.getElementById(
    "downloadFeaturesButton"
  );

const progressElement =
  document.getElementById("progress");

const hitsElement =
  document.getElementById("hits");

const missesElement =
  document.getElementById("misses");

const instructions =
  document.getElementById("instructions");

const results =
  document.getElementById("results");


let currentTrial = 0;

let hits = 0;
let misses = 0;

let trialStartTime = 0;

let currentTarget = null;

let lastSampleTime = 0;

let lastPoint = null;


const samples = [];

const trials = [];

let trialStartSampleIndex = 0;
let currentTrialMisses = 0;

const HESITATION_VELOCITY = 60;
const MIN_MOVEMENT_FOR_DIRECTION = 2;

function placeTarget() {

  const areaRect =
    testArea.getBoundingClientRect();


  const size =
    targetSizes[
      Math.floor(
        Math.random() *
        targetSizes.length
      )
    ];


  const margin = 30;


  const maxX =
    areaRect.width -
    size -
    margin * 2;


  const maxY =
    areaRect.height -
    size -
    margin * 2;


  const x =
    margin +
    Math.random() * maxX;


  const y =
    margin +
    Math.random() * maxY;


  target.style.width =
    `${size}px`;

  target.style.height =
    `${size}px`;

  target.style.left =
    `${x}px`;

  target.style.top =
    `${y}px`;


  target.hidden = false;


  currentTarget = {
    x,
    y,

    centerX:
      areaRect.left +
      x +
      size / 2,

    centerY:
      areaRect.top +
      y +
      size / 2,

    size
  };


  trialStartTime =
    performance.now();

  trialStartSampleIndex = samples.length;

currentTrialMisses = 0;

lastPoint = null;

}

function startCalibration() {

  currentTrial = 1;

  hits = 0;
  misses = 0;

  samples.length = 0;
  trials.length = 0;

  lastPoint = null;


  progressElement.textContent =
    `0 / ${TOTAL_TRIALS}`;

  hitsElement.textContent = "0";
  missesElement.textContent = "0";


  results.hidden = true;

  instructions.hidden = true;

  downloadRawButton.disabled = true;

  downloadFeaturesButton.disabled = true;


  placeTarget();

}

startButton.addEventListener(
  "click",
  startCalibration
);

document.addEventListener(
  "mousemove",
  (event) => {

    if (!currentTarget) {
      return;
    }


    const now =
      performance.now();


    // Sample roughly 20 times per second
    if (now - lastSampleTime < 50) {
      return;
    }


    lastSampleTime = now;


    let velocity = 0;
let acceleration = 0;
let dx = 0;
let dy = 0;


if (lastPoint) {

  dx =
    event.clientX -
    lastPoint.x;

  dy =
    event.clientY -
    lastPoint.y;


  const distance =
    Math.sqrt(
      dx * dx +
      dy * dy
    );


  const dt =
    (now -
    lastPoint.time) /
    1000;


  if (dt > 0) {

    velocity =
      distance / dt;


    if (
      lastPoint.velocity !==
      undefined
    ) {

      acceleration =
        (velocity -
        lastPoint.velocity) /
        dt;

    }

  }

}

    const dxToTarget =
      currentTarget.centerX -
      event.clientX;


    const dyToTarget =
      currentTarget.centerY -
      event.clientY;


    const distanceToTarget =
      Math.sqrt(
        dxToTarget * dxToTarget +
        dyToTarget * dyToTarget
      );


    samples.push({

  trial:
    currentTrial,

  timestamp:
    now,

  x:
    event.clientX,

  y:
    event.clientY,

  dx,

  dy,

  velocity,

  acceleration,

  targetX:
    currentTarget.centerX,

  targetY:
    currentTarget.centerY,

  targetSize:
    currentTarget.size,

  distanceToTarget

});

    lastPoint = {

      x: event.clientX,

      y: event.clientY,

      time: now,

      velocity

    };

  }
);

function calculateTrialFeatures(
  trialSamples,
  clickX,
  clickY
) {

  if (trialSamples.length === 0) {
    return null;
  }


  let pathDistance = 0;

  let velocityTotal = 0;
  let peakVelocity = 0;

  let accelerationTotal = 0;
  let peakAcceleration = 0;

  let hesitationMs = 0;

  let directionReversals = 0;

  let overshoots = 0;


  // -----------------------
  // PATH + SPEED FEATURES
  // -----------------------

  for (
    let i = 0;
    i < trialSamples.length;
    i++
  ) {

    const sample =
      trialSamples[i];


    velocityTotal +=
      sample.velocity;


    peakVelocity =
      Math.max(
        peakVelocity,
        sample.velocity
      );


    const absoluteAcceleration =
      Math.abs(
        sample.acceleration
      );


    accelerationTotal +=
      absoluteAcceleration;


    peakAcceleration =
      Math.max(
        peakAcceleration,
        absoluteAcceleration
      );


    if (i > 0) {

      const previous =
        trialSamples[i - 1];


      const dx =
        sample.x -
        previous.x;


      const dy =
        sample.y -
        previous.y;


      pathDistance +=
        Math.sqrt(
          dx * dx +
          dy * dy
        );


      const dt =
        sample.timestamp -
        previous.timestamp;


      if (
        sample.velocity <
        HESITATION_VELOCITY
      ) {

        hesitationMs += dt;

      }

    }

  }


  // -----------------------
  // DIRECTION REVERSALS
  // -----------------------

  for (
    let i = 2;
    i < trialSamples.length;
    i++
  ) {

    const p1 =
      trialSamples[i - 2];

    const p2 =
      trialSamples[i - 1];

    const p3 =
      trialSamples[i];


    const v1x =
      p2.x - p1.x;

    const v1y =
      p2.y - p1.y;


    const v2x =
      p3.x - p2.x;

    const v2y =
      p3.y - p2.y;


    const length1 =
      Math.sqrt(
        v1x * v1x +
        v1y * v1y
      );


    const length2 =
      Math.sqrt(
        v2x * v2x +
        v2y * v2y
      );


    if (
      length1 <
      MIN_MOVEMENT_FOR_DIRECTION ||
      length2 <
      MIN_MOVEMENT_FOR_DIRECTION
    ) {

      continue;

    }


    const dotProduct =
      v1x * v2x +
      v1y * v2y;


    if (dotProduct < 0) {

      directionReversals++;

    }

  }


  // -----------------------
  // OVERSHOOT DETECTION
  // -----------------------

  let closestDistance =
    Infinity;

  let inOvershoot = false;


  const overshootThreshold =
    Math.max(
      6,
      currentTarget.size * 0.15
    );


  for (
    const sample of
    trialSamples
  ) {

    if (
      sample.distanceToTarget <
      closestDistance
    ) {

      closestDistance =
        sample.distanceToTarget;

      inOvershoot = false;

    }

    else if (
      sample.distanceToTarget >
      closestDistance +
      overshootThreshold
    ) {

      if (!inOvershoot) {

        overshoots++;

        inOvershoot = true;

      }

    }

  }


  // -----------------------
  // PATH EFFICIENCY
  // -----------------------

  const firstSample =
    trialSamples[0];


  const straightDx =
    currentTarget.centerX -
    firstSample.x;


  const straightDy =
    currentTarget.centerY -
    firstSample.y;


  const straightLineDistance =
    Math.sqrt(
      straightDx * straightDx +
      straightDy * straightDy
    );


  let pathEfficiency = 0;


  if (pathDistance > 0) {

    pathEfficiency =
      straightLineDistance /
      pathDistance;

  }


  pathEfficiency =
    Math.min(
      pathEfficiency,
      1
    );


  // -----------------------
  // CLICK OFFSET
  // -----------------------

  const clickDx =
    clickX -
    currentTarget.centerX;


  const clickDy =
    clickY -
    currentTarget.centerY;


  const clickOffset =
    Math.sqrt(
      clickDx * clickDx +
      clickDy * clickDy
    );


  // -----------------------
  // AVERAGES
  // -----------------------

  const averageVelocity =
    velocityTotal /
    trialSamples.length;


  const averageAcceleration =
    accelerationTotal /
    trialSamples.length;


  return {

    averageVelocity,

    peakVelocity,

    averageAcceleration,

    peakAcceleration,

    directionReversals,

    overshoots,

    hesitationMs,

    pathDistance,

    straightLineDistance,

    pathEfficiency,

    clickOffset,

    sampleCount:
      trialSamples.length

  };

}

testArea.addEventListener(
  "click",
  (event) => {

    if (!currentTarget) {
      return;
    }


    if (event.target === target) {
      return;
    }

    misses++;

    currentTrialMisses++;

    missesElement.textContent =
      misses;

  }
);

target.addEventListener(
  "click",
  (event) => {

    event.stopPropagation();


    if (!currentTarget) {
      return;
    }


    hits++;


    const now =
      performance.now();


    const duration =
      now -
      trialStartTime;


    const dx =
      event.clientX -
      currentTarget.centerX;


    const dy =
      event.clientY -
      currentTarget.centerY;


    const clickOffset =
      Math.sqrt(
        dx * dx +
        dy * dy
      );


    const trialSamples =
  samples.slice(
    trialStartSampleIndex
  );


const features =
  calculateTrialFeatures(
    trialSamples,
    event.clientX,
    event.clientY
  );


if (features) {

  trials.push({

    trial:
      currentTrial,

    targetSize:
      currentTarget.size,

    targetX:
      currentTarget.centerX,

    targetY:
      currentTarget.centerY,

    durationMs:
      duration,

    misses:
      currentTrialMisses,

    clickX:
      event.clientX,

    clickY:
      event.clientY,

    ...features

  });

}


    hitsElement.textContent =
      hits;


    progressElement.textContent =
      `${currentTrial} / ${TOTAL_TRIALS}`;


    if (
      currentTrial >=
      TOTAL_TRIALS
    ) {

      finishCalibration();

      return;

    }


    currentTrial++;

    lastPoint = null;

    placeTarget();

  }
);

function finishCalibration() {

  target.hidden = true;

  currentTarget = null;


  const totalClicks =
    hits + misses;


  const accuracy =
    totalClicks === 0
      ? 0
      : hits / totalClicks;


  const averageTime =
    trials.reduce(
      (sum, trial) =>
        sum +
        trial.duration,
      0
    ) /
    trials.length;


  const averageOffset =
    trials.reduce(
      (sum, trial) =>
        sum +
        trial.clickOffset,
      0
    ) /
    trials.length;


  document.getElementById(
    "accuracyResult"
  ).textContent =
    `${(
      accuracy * 100
    ).toFixed(1)}%`;


  document.getElementById(
    "timeResult"
  ).textContent =
    `${(
      averageTime /
      1000
    ).toFixed(2)} s`;


  document.getElementById(
    "offsetResult"
  ).textContent =
    `${averageOffset.toFixed(1)} px`;


  document.getElementById(
    "samplesResult"
  ).textContent =
    samples.length;


  results.hidden = false;

  instructions.hidden = false;

  instructions.textContent =
    "Calibration complete!";


  downloadRawButton.disabled =
    false;

  downloadFeaturesButton.disabled =
    false;

}

function downloadRawCSV() {

  const headers = [

    "trial",

    "timestamp",

    "x",

    "y",

    "dx",

    "dy",

    "velocity",

    "acceleration",

    "targetX",

    "targetY",

    "targetSize",

    "distanceToTarget"

  ];

  function downloadFeatureCSV() {

  const headers = [

    "trial",

    "targetSize",

    "targetX",

    "targetY",

    "durationMs",

    "misses",

    "clickX",

    "clickY",

    "averageVelocity",

    "peakVelocity",

    "averageAcceleration",

    "peakAcceleration",

    "directionReversals",

    "overshoots",

    "hesitationMs",

    "pathDistance",

    "straightLineDistance",

    "pathEfficiency",

    "clickOffset",

    "sampleCount"

  ];


  const rows =
    trials.map(
      trial => [

        trial.trial,

        trial.targetSize,

        trial.targetX,

        trial.targetY,

        trial.durationMs,

        trial.misses,

        trial.clickX,

        trial.clickY,

        trial.averageVelocity,

        trial.peakVelocity,

        trial.averageAcceleration,

        trial.peakAcceleration,

        trial.directionReversals,

        trial.overshoots,

        trial.hesitationMs,

        trial.pathDistance,

        trial.straightLineDistance,

        trial.pathEfficiency,

        trial.clickOffset,

        trial.sampleCount

      ]
    );


  const csv = [

    headers.join(","),

    ...rows.map(
      row =>
        row.join(",")
    )

  ].join("\n");


  const blob =
    new Blob(
      [csv],
      {
        type:
          "text/csv"
      }
    );


  const url =
    URL.createObjectURL(
      blob
    );


  const link =
    document.createElement(
      "a"
    );


  link.href = url;

  link.download =
    "steadyui-trial-features.csv";


  link.click();


  URL.revokeObjectURL(
    url
  );

}

  const rows =
  samples.map(
    sample => [

      sample.trial,

      sample.timestamp,

      sample.x,

      sample.y,

      sample.dx,

      sample.dy,

      sample.velocity,

      sample.acceleration,

      sample.targetX,

      sample.targetY,

      sample.targetSize,

      sample.distanceToTarget

    ]
  );


  const csv = [

    headers.join(","),

    ...rows.map(
      row =>
        row.join(",")
    )

  ].join("\n");


  const blob =
    new Blob(
      [csv],
      {
        type:
          "text/csv"
      }
    );


  const url =
    URL.createObjectURL(
      blob
    );


  const link =
    document.createElement(
      "a"
    );


  link.href = url;

  link.download =
    "steadyui-raw-samples.csv";


  link.click();


  URL.revokeObjectURL(
    url
  );

}

downloadRawButton.addEventListener(
  "click",
  downloadRawCSV
);


downloadFeaturesButton.addEventListener(
  "click",
  downloadFeatureCSV
);