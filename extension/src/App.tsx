import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import './App.css'

type FilterMode = 'Deadband' | 'Smooth' | 'PassThrough' | 'FlickBypass'

type TelemetryFrame = {
  timestampUs: number
  rawDx: number
  rawDy: number
  correctedDx: number
  correctedDy: number
  mode: FilterMode
}

type CalibrationTarget = {
  x: number
  y: number
  size: number
  label: string
}

type CalibrationSample = {
  tMs: number
  trial: number
  x: number
  y: number
  areaWidth: number
  areaHeight: number
  targetX: number
  targetY: number
  targetSize: number
}

type CalibrationTrial = {
  trial: number
  target: CalibrationTarget
  clickX: number
  clickY: number
  elapsedMs: number
  hit: boolean
}

type CalibrationPhase = 'idle' | 'targets' | 'complete'

type SteadyUIProfile = {
  hitboxPadding: number
  pathEfficiency: number
  misses: number
  averageTargetTimeMs: number
  calibratedAt: string
}

declare const chrome:
  | {
      storage?: {
        local?: {
          get: (
            keys: string[] | string,
            callback: (result: Record<string, unknown>) => void,
          ) => void
          set: (
            items: Record<string, unknown>,
            callback?: () => void,
          ) => void
        }
      }
    }
  | undefined

const STORAGE_KEY = 'steadyUIProfile'

const DEMO_VALUES: [number, number, number, number, number, FilterMode][] = [
  [1, 1, 0, 0, 0, 'Deadband'],
  [2, -1, 1, 0, 0, 'Deadband'],
  [3, 3, 0, 3, 0, 'PassThrough'],
  [4, -2, 1, 0, 0, 'Smooth'],
  [5, 2, 0, 1, 0, 'Smooth'],
  [6, 4, 1, 4, 1, 'PassThrough'],
  [7, 9, -2, 9, -2, 'FlickBypass'],
  [8, 1, 1, 0, 0, 'Deadband'],
  [9, -3, 1, -2, 1, 'Smooth'],
  [10, 5, 2, 5, 2, 'PassThrough'],
]

const DEMO_FRAMES: TelemetryFrame[] = DEMO_VALUES.map(
  ([step, rawDx, rawDy, correctedDx, correctedDy, mode]) => ({
    timestampUs: 1_790_486_547_200_000 + step * 8_000,
    rawDx,
    rawDy,
    correctedDx,
    correctedDy,
    mode,
  }),
)

const modeLabel: Record<FilterMode, string> = {
  Deadband: 'Suppressed',
  Smooth: 'Smoothed',
  PassThrough: 'Passed through',
  FlickBypass: 'Flick preserved',
}

const magnitude = (dx: number, dy: number) => Math.hypot(dx, dy)

const TARGETS: CalibrationTarget[] = [
  { x: 50, y: 50, size: 58, label: 'Center check' },
  { x: 18, y: 24, size: 46, label: 'Target 1' },
  { x: 82, y: 26, size: 40, label: 'Target 2' },
  { x: 26, y: 72, size: 34, label: 'Target 3' },
  { x: 74, y: 68, size: 30, label: 'Target 4' },
  { x: 50, y: 20, size: 38, label: 'Target 5' },
  { x: 50, y: 81, size: 42, label: 'Target 6' },
]

function deriveStandaloneProfile(
  samples: CalibrationSample[],
  trials: CalibrationTrial[],
): SteadyUIProfile {
  const efficiencies: number[] = []

  for (let trialIndex = 0; trialIndex < TARGETS.length; trialIndex += 1) {
    const path = samples.filter((sample) => sample.trial === trialIndex)

    if (path.length < 2) {
      continue
    }

    let traveled = 0

    for (let i = 1; i < path.length; i += 1) {
      traveled += Math.hypot(
        path[i].x - path[i - 1].x,
        path[i].y - path[i - 1].y,
      )
    }

    const first = path[0]
    const direct = Math.hypot(
      first.targetX - first.x,
      first.targetY - first.y,
    )

    if (direct > 10) {
      efficiencies.push(Math.min(3, traveled / direct))
    }
  }

  const pathEfficiency =
    efficiencies.length > 0
      ? efficiencies.reduce((sum, value) => sum + value, 0) /
        efficiencies.length
      : 1

  const misses = trials.filter((trial) => !trial.hit).length
  const successfulTrials = trials.filter((trial) => trial.hit)

  const averageTargetTimeMs =
    successfulTrials.length > 0
      ? successfulTrials.reduce(
          (sum, trial) => sum + trial.elapsedMs,
          0,
        ) / successfulTrials.length
      : 0

  const hitboxPadding = Math.round(
    Math.max(
      10,
      Math.min(
        36,
        10 + (pathEfficiency - 1) * 18 + misses * 3,
      ),
    ),
  )

  return {
    hitboxPadding,
    pathEfficiency,
    misses,
    averageTargetTimeMs,
    calibratedAt: new Date().toISOString(),
  }
}

function samplePath(samples: number[]) {
  return samples
    .map(
      (value, index) =>
        `${index === 0 ? 'M' : 'L'} ${
          (index / Math.max(samples.length - 1, 1)) * 530
        } ${60 - value * 9}`,
    )
    .join(' ')
}

function saveProfile(profile: SteadyUIProfile) {
  if (
    typeof chrome !== 'undefined' &&
    chrome.storage?.local?.set
  ) {
    chrome.storage.local.set({
      [STORAGE_KEY]: profile,
    })
    return
  }

  window.localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify(profile),
  )
}

function loadProfile(
  callback: (profile: SteadyUIProfile | null) => void,
) {
  if (
    typeof chrome !== 'undefined' &&
    chrome.storage?.local?.get
  ) {
    chrome.storage.local.get([STORAGE_KEY], (result) => {
      const profile = result[STORAGE_KEY]

      if (
        profile &&
        typeof profile === 'object' &&
        'hitboxPadding' in profile
      ) {
        callback(profile as SteadyUIProfile)
      } else {
        callback(null)
      }
    })
    return
  }

  try {
    const saved = window.localStorage.getItem(STORAGE_KEY)
    callback(
      saved ? (JSON.parse(saved) as SteadyUIProfile) : null,
    )
  } catch {
    callback(null)
  }
}

function App() {
  const [frames, setFrames] = useState<TelemetryFrame[]>(
    DEMO_FRAMES.slice(0, 1),
  )
  const [running, setRunning] = useState(false)
  const [demoIndex, setDemoIndex] = useState(1)

  const [calibrationActive, setCalibrationActive] =
    useState(false)
  const [calibrationPhase, setCalibrationPhase] =
    useState<CalibrationPhase>('idle')
  const [calibrationTrial, setCalibrationTrial] = useState(0)
  const [calibrationStartedAt, setCalibrationStartedAt] =
    useState(0)
  const [calibrationSamples, setCalibrationSamples] = useState<
    CalibrationSample[]
  >([])
  const [calibrationTrials, setCalibrationTrials] = useState<
    CalibrationTrial[]
  >([])
  const [calibrationStatus, setCalibrationStatus] = useState(
    'Ready for standalone calibration',
  )
  const [activeProfile, setActiveProfile] =
    useState<SteadyUIProfile | null>(null)

  const lastCalibrationSampleAt = useRef(0)

  useEffect(() => {
    loadProfile((profile) => {
      setActiveProfile(profile)

      if (profile) {
        setCalibrationStatus(
          `Saved profile loaded · ${profile.hitboxPadding}px assistance`,
        )
      }
    })
  }, [])

  useEffect(() => {
    if (!running) {
      return undefined
    }

    const interval = window.setInterval(() => {
      setFrames((current) => [
        ...current.slice(-59),
        DEMO_FRAMES[demoIndex],
      ])
      setDemoIndex(
        (index) => (index + 1) % DEMO_FRAMES.length,
      )
    }, 650)

    return () => window.clearInterval(interval)
  }, [demoIndex, running])

  const current = frames.at(-1) ?? DEMO_FRAMES[0]

  const counts = useMemo(
    () =>
      frames.reduce<Record<FilterMode, number>>(
        (total, frame) => ({
          ...total,
          [frame.mode]: total[frame.mode] + 1,
        }),
        {
          Deadband: 0,
          Smooth: 0,
          PassThrough: 0,
          FlickBypass: 0,
        },
      ),
    [frames],
  )

  const rawTravel = frames.reduce(
    (total, frame) =>
      total + magnitude(frame.rawDx, frame.rawDy),
    0,
  )

  const correctedTravel = frames.reduce(
    (total, frame) =>
      total +
      magnitude(frame.correctedDx, frame.correctedDy),
    0,
  )

  const reduction =
    rawTravel === 0
      ? 0
      : Math.max(
          0,
          (1 - correctedTravel / rawTravel) * 100,
        )

  const startDemo = () => {
    setFrames(DEMO_FRAMES.slice(0, 1))
    setDemoIndex(1)
    setRunning(true)
  }

  const beginCalibration = () => {
    setCalibrationSamples([])
    setCalibrationTrials([])
    setCalibrationTrial(0)
    setCalibrationStartedAt(performance.now())
    lastCalibrationSampleAt.current = 0
    setCalibrationPhase('targets')
    setCalibrationActive(true)
    setCalibrationStatus('Calibration active')
  }

  const captureCalibrationSample = (
    event: MouseEvent<HTMLDivElement>,
  ) => {
    if (
      !calibrationActive ||
      calibrationPhase !== 'targets'
    ) {
      return
    }

    const now = performance.now()

    if (now - lastCalibrationSampleAt.current < 16) {
      return
    }

    lastCalibrationSampleAt.current = now

    const bounds =
      event.currentTarget.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]

    const sample: CalibrationSample = {
      tMs: now - calibrationStartedAt,
      trial: calibrationTrial,
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
      areaWidth: bounds.width,
      areaHeight: bounds.height,
      targetX: (bounds.width * target.x) / 100,
      targetY: (bounds.height * target.y) / 100,
      targetSize: target.size,
    }

    setCalibrationSamples((samples) => [
      ...samples,
      sample,
    ])
  }

  const recordTargetClick = (
    event: MouseEvent<HTMLButtonElement>,
  ) => {
    event.stopPropagation()

    const bounds =
      event.currentTarget.parentElement!.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]

    const trial: CalibrationTrial = {
      trial: calibrationTrial,
      target,
      clickX: event.clientX - bounds.left,
      clickY: event.clientY - bounds.top,
      elapsedMs:
        performance.now() - calibrationStartedAt,
      hit: true,
    }

    const nextTrials = [...calibrationTrials, trial]
    setCalibrationTrials(nextTrials)

    if (calibrationTrial + 1 === TARGETS.length) {
      const profile = deriveStandaloneProfile(
        calibrationSamples,
        nextTrials,
      )

      saveProfile(profile)
      setActiveProfile(profile)
      setCalibrationStatus(
        `Personalized profile saved · ${profile.hitboxPadding}px adaptive assistance`,
      )
      setCalibrationActive(false)
      setCalibrationPhase('complete')
      return
    }

    setCalibrationTrial((trialIndex) => trialIndex + 1)
    setCalibrationStartedAt(performance.now())
    lastCalibrationSampleAt.current = 0
  }

  const recordMiss = (
    event: MouseEvent<HTMLDivElement>,
  ) => {
    if (
      !calibrationActive ||
      calibrationPhase !== 'targets' ||
      event.target !== event.currentTarget
    ) {
      return
    }

    const bounds =
      event.currentTarget.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]

    const trial: CalibrationTrial = {
      trial: calibrationTrial,
      target,
      clickX: event.clientX - bounds.left,
      clickY: event.clientY - bounds.top,
      elapsedMs:
        performance.now() - calibrationStartedAt,
      hit: false,
    }

    setCalibrationTrials((trials) => [
      ...trials,
      trial,
    ])
  }

  const downloadCalibration = () => {
    const payload = {
      schemaVersion: 2,
      kind: 'steadyui_standalone_calibration',
      capturedAt: new Date().toISOString(),
      coordinateSpace: 'target_area_relative_pixels',
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio,
      },
      profile: activeProfile,
      trials: calibrationTrials,
      samples: calibrationSamples,
    }

    const url = URL.createObjectURL(
      new Blob([JSON.stringify(payload, null, 2)], {
        type: 'application/json',
      }),
    )

    const link = document.createElement('a')
    link.href = url
    link.download = 'steadyui-calibration.json'
    link.click()
    URL.revokeObjectURL(url)
  }

  const successfulHits = calibrationTrials.filter(
    (trial) => trial.hit,
  ).length

  const totalAttempts = calibrationTrials.length

  const accuracy =
    totalAttempts === 0
      ? 0
      : (successfulHits / totalAttempts) * 100

  return (
    <main className="console">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark">≈</span>
          <div>
            <p className="eyebrow">
              SteadyUI / standalone accessibility
            </p>
            <h1>Adaptive interaction console</h1>
          </div>
        </div>

        <div className="session-controls">
          <span className="connection demo">
            <i />
            Standalone
          </span>

          <button
            className="secondary"
            type="button"
            onClick={() => setRunning(false)}
            disabled={!running}
          >
            Pause preview
          </button>

          <button
            className="secondary"
            type="button"
            onClick={beginCalibration}
          >
            {calibrationActive
              ? 'Restart calibration'
              : 'Start calibration'}
          </button>

          <button type="button" onClick={startDemo}>
            Run validation preview
          </button>
        </div>
      </header>

      <section
        className="pipeline"
        aria-label="SteadyUI accessibility pipeline"
      >
        {[
          [
            'Pointer movement',
            'Browser mouse trajectory',
            'ready',
          ],
          [
            'Intent scoring',
            'Direction + distance + approach',
            'ready',
          ],
          [
            'Adaptive hitbox',
            activeProfile
              ? `${activeProfile.hitboxPadding}px personalized padding`
              : 'Calibrate to personalize',
            'active',
          ],
          [
            'Click rescue',
            'Near-miss activation',
            'ready',
          ],
        ].map(([title, detail, state], index) => (
          <div className="pipeline-piece" key={title}>
            <div className={`node ${state}`}>
              <span>{index + 1}</span>
            </div>
            <div>
              <strong>{title}</strong>
              <small>{detail}</small>
            </div>
            {index < 3 && <div className="arrow">→</div>}
          </div>
        ))}
      </section>

      <section className="panel calibration-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">
              Personalized target calibration
            </p>
            <h2>
              Learn how precisely you move and click
            </h2>
          </div>

          <div className="calibration-actions">
            <span className="sample-count">
              {calibrationStatus} ·{' '}
              {calibrationSamples.length} path samples ·{' '}
              {successfulHits}/{TARGETS.length} targets
            </span>

            {calibrationTrials.length > 0 && (
              <button
                className="secondary"
                type="button"
                onClick={downloadCalibration}
              >
                Download calibration JSON
              </button>
            )}
          </div>
        </div>

        <p className="calibration-copy">
          Click seven targets that become progressively
          smaller and move around the calibration area.
          SteadyUI measures path efficiency, misses, and
          target timing, then chooses a personalized
          adaptive hitbox size.
        </p>

        <div
          className={`target-area ${
            calibrationActive ? 'active' : ''
          }`}
          onMouseMove={captureCalibrationSample}
          onClick={recordMiss}
        >
          {calibrationPhase === 'idle' && (
            <div className="calibration-instructions">
              <strong>Ready to personalize SteadyUI.</strong>
              <span>
                No native app is required. Complete the
                target exercise to generate a browser-only
                accessibility profile.
              </span>
              <button
                type="button"
                onClick={beginCalibration}
              >
                Begin calibration
              </button>
            </div>
          )}

          {calibrationActive &&
            calibrationPhase === 'targets' && (
              <>
                <div className="target-progress">
                  {TARGETS[calibrationTrial].label} ·{' '}
                  {calibrationTrial + 1}/{TARGETS.length}
                </div>

                <button
                  className="calibration-target"
                  type="button"
                  aria-label={
                    TARGETS[calibrationTrial].label
                  }
                  onClick={recordTargetClick}
                  style={{
                    left: `${TARGETS[calibrationTrial].x}%`,
                    top: `${TARGETS[calibrationTrial].y}%`,
                    width:
                      TARGETS[calibrationTrial].size,
                    height:
                      TARGETS[calibrationTrial].size,
                  }}
                >
                  <span />
                </button>
              </>
            )}

          {calibrationPhase === 'complete' && (
            <div className="calibration-instructions complete">
              <strong>
                Calibration complete.
              </strong>
              <span>
                {activeProfile
                  ? `${activeProfile.hitboxPadding}px adaptive padding selected from ${activeProfile.pathEfficiency.toFixed(
                      2,
                    )}× path efficiency and ${activeProfile.misses} misses.`
                  : 'Profile saved.'}
              </span>
              <button
                type="button"
                onClick={beginCalibration}
              >
                Run again
              </button>
            </div>
          )}
        </div>

        <div className="calibration-footnote">
          <span>
            Browser-only calibration: trajectory, timing,
            hits, and misses
          </span>
          <span>
            Saved locally with Chrome extension storage
          </span>
        </div>
      </section>

      <section className="dashboard-grid">
        <article className="panel movement-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">
                Validation preview
              </p>
              <h2>
                Raw vs assisted movement concept
              </h2>
            </div>
            <span
              className={`mode-pill ${current.mode}`}
            >
              {modeLabel[current.mode]}
            </span>
          </div>

          <div className="delta-pair">
            <div className="delta raw">
              <span>Raw input</span>
              <strong>
                {current.rawDx > 0 ? '+' : ''}
                {current.rawDx},{' '}
                {current.rawDy > 0 ? '+' : ''}
                {current.rawDy}
              </strong>
              <small>dx, dy</small>
            </div>

            <div className="transfer">→</div>

            <div className="delta corrected">
              <span>Assisted output</span>
              <strong>
                {current.correctedDx > 0 ? '+' : ''}
                {current.correctedDx},{' '}
                {current.correctedDy > 0 ? '+' : ''}
                {current.correctedDy}
              </strong>
              <small>demo signal</small>
            </div>
          </div>

          <div className="signal-chart">
            <div className="chart-legend">
              <span>
                <i className="raw-dot" />
                Raw dx
              </span>
              <span>
                <i className="corrected-dot" />
                Assisted dx
              </span>
            </div>

            <svg
              viewBox="0 0 530 120"
              preserveAspectRatio="none"
            >
              <path
                className="baseline"
                d="M 0 60 L 530 60"
              />
              <path
                className="raw-line"
                d={samplePath(
                  frames
                    .slice(-18)
                    .map((frame) => frame.rawDx),
                )}
              />
              <path
                className="corrected-line"
                d={samplePath(
                  frames
                    .slice(-18)
                    .map(
                      (frame) =>
                        frame.correctedDx,
                    ),
                )}
              />
            </svg>
          </div>
        </article>

        <article className="panel profile-panel">
          <p className="eyebrow">Active profile</p>
          <h2>
            {activeProfile
              ? 'Personalized'
              : 'Not calibrated'}
          </h2>

          <dl>
            <div>
              <dt>Adaptive padding</dt>
              <dd>
                {activeProfile
                  ? `${activeProfile.hitboxPadding}px`
                  : '—'}
              </dd>
            </div>
            <div>
              <dt>Path efficiency</dt>
              <dd>
                {activeProfile
                  ? `${activeProfile.pathEfficiency.toFixed(
                      2,
                    )}×`
                  : '—'}
              </dd>
            </div>
            <div>
              <dt>Misses</dt>
              <dd>
                {activeProfile
                  ? activeProfile.misses
                  : '—'}
              </dd>
            </div>
            <div>
              <dt>Avg target time</dt>
              <dd>
                {activeProfile
                  ? `${Math.round(
                      activeProfile.averageTargetTimeMs,
                    )} ms`
                  : '—'}
              </dd>
            </div>
          </dl>

          <p className="profile-note">
            The same profile is read by the content script
            to scale adaptive hitboxes across webpages.
          </p>
        </article>

        <article className="panel metrics-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">
                Calibration result
              </p>
              <h2>Target accuracy</h2>
            </div>
            <strong className="metric">
              {accuracy.toFixed(0)}%
            </strong>
          </div>

          <div className="meter">
            <span
              style={{ width: `${accuracy}%` }}
            />
          </div>

          <div className="metric-row">
            <span>
              Hits <b>{successfulHits}</b>
            </span>
            <span>
              Misses{' '}
              <b>
                {
                  calibrationTrials.filter(
                    (trial) => !trial.hit,
                  ).length
                }
              </b>
            </span>
          </div>

          <p className="muted">
            Misses and inefficient pointer paths increase
            the amount of assistance SteadyUI provides.
          </p>
        </article>

        <article className="panel modes-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">
                Runtime assistance
              </p>
              <h2>How SteadyUI helps</h2>
            </div>
          </div>

          <div className="mode-list">
            <div className="mode-row">
              <span className="mode-swatch Smooth" />
              <span>Intent scoring</span>
              <div className="mode-bar">
                <i style={{ width: '100%' }} />
              </div>
              <b>Live</b>
            </div>

            <div className="mode-row">
              <span className="mode-swatch Deadband" />
              <span>Adaptive hitboxes</span>
              <div className="mode-bar">
                <i
                  style={{
                    width: activeProfile
                      ? '100%'
                      : '45%',
                  }}
                />
              </div>
              <b>
                {activeProfile
                  ? `${activeProfile.hitboxPadding}px`
                  : 'Default'}
              </b>
            </div>

            <div className="mode-row">
              <span className="mode-swatch FlickBypass" />
              <span>Near-miss rescue</span>
              <div className="mode-bar">
                <i style={{ width: '100%' }} />
              </div>
              <b>Live</b>
            </div>
          </div>
        </article>
      </section>

      <section className="panel event-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">
              Calibration attempts
            </p>
            <h2>Latest target interactions</h2>
          </div>
          <span className="safety">
            <i />
            Browser-only · no native bridge
          </span>
        </div>

        <div className="event-table">
          <div className="event-head">
            <span>Target</span>
            <span>Result</span>
            <span>Time</span>
            <span>Click</span>
            <span>Assist</span>
          </div>

          {calibrationTrials
            .slice(-6)
            .reverse()
            .map((trial, index) => (
              <div
                className="event-row"
                key={`${trial.trial}-${trial.elapsedMs}-${index}`}
              >
                <span className="timestamp">
                  {trial.target.label}
                </span>
                <span>
                  {trial.hit ? 'Hit' : 'Miss'}
                </span>
                <span>
                  {Math.round(trial.elapsedMs)} ms
                </span>
                <span>
                  ({Math.round(trial.clickX)},{' '}
                  {Math.round(trial.clickY)})
                </span>
                <span>
                  {activeProfile
                    ? `${activeProfile.hitboxPadding}px`
                    : 'Default'}
                </span>
              </div>
            ))}

          {calibrationTrials.length === 0 && (
            <div className="event-row">
              <span className="timestamp">
                No calibration yet
              </span>
              <span>—</span>
              <span>—</span>
              <span>—</span>
              <span>Default</span>
            </div>
          )}
        </div>
      </section>

      <section className="panel event-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">
              Demo signal
            </p>
            <h2>Validation preview decisions</h2>
          </div>
          <span className="sample-count">
            {frames.length} samples
          </span>
        </div>

        <div className="mode-list">
          {(Object.keys(counts) as FilterMode[]).map(
            (mode) => (
              <div
                className="mode-row"
                key={mode}
              >
                <span
                  className={`mode-swatch ${mode}`}
                />
                <span>{modeLabel[mode]}</span>
                <div className="mode-bar">
                  <i
                    style={{
                      width: `${
                        (counts[mode] /
                          Math.max(
                            frames.length,
                            1,
                          )) *
                        100
                      }%`,
                    }}
                  />
                </div>
                <b>{counts[mode]}</b>
              </div>
            ),
          )}
        </div>

        <p className="muted">
          The preview is illustrative only. The standalone
          product’s live browser features are intent scoring,
          adaptive hitboxes, and near-miss click rescue.
          Current preview movement reduction:{' '}
          {reduction.toFixed(1)}%.
        </p>
      </section>
    </main>
  )
}

export default App
