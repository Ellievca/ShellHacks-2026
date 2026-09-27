import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import './App.css'

type FilterMode = 'Deadband' | 'Smooth' | 'PassThrough' | 'FlickBypass'
type TelemetryFrame = { timestampUs: number; rawDx: number; rawDy: number; correctedDx: number; correctedDy: number; mode: FilterMode }
type TelemetryEvent = CustomEvent<TelemetryFrame>
type CalibrationTarget = { x: number; y: number; size: number; label: string }
type CalibrationSample = {
  tMs: number; trial: number; x: number; y: number; areaWidth: number; areaHeight: number; targetX: number; targetY: number; targetSize: number
  telemetry: Omit<TelemetryFrame, 'timestampUs'>; telemetrySource: 'demo' | 'daemon'
}
type CalibrationTrial = { trial: number; target: CalibrationTarget; clickX: number; clickY: number; elapsedMs: number; hit: boolean }
type CalibrationPhase = 'idle' | 'still' | 'slow' | 'flick' | 'deriving' | 'targets' | 'complete'

declare global { interface WindowEventMap { 'zerotremor:telemetry': TelemetryEvent } }

const DEMO_VALUES: [number, number, number, number, number, FilterMode][] = [
  [1, 1, 0, 0, 0, 'Deadband'], [2, -1, 1, 0, 0, 'Deadband'], [3, 3, 0, 3, 0, 'PassThrough'],
  [4, -2, 1, 0, 0, 'Smooth'], [5, 2, 0, 1, 0, 'Smooth'], [6, 4, 1, 4, 1, 'PassThrough'],
  [7, 9, -2, 9, -2, 'FlickBypass'], [8, 1, 1, 0, 0, 'Deadband'], [9, -3, 1, -2, 1, 'Smooth'], [10, 5, 2, 5, 2, 'PassThrough'],
]
const DEMO_FRAMES: TelemetryFrame[] = DEMO_VALUES.map(([step, rawDx, rawDy, correctedDx, correctedDy, mode]) => ({ timestampUs: 1_790_486_547_200_000 + step * 8_000, rawDx, rawDy, correctedDx, correctedDy, mode }))

const modeLabel: Record<FilterMode, string> = { Deadband: 'Suppressed', Smooth: 'Smoothed', PassThrough: 'Passed through', FlickBypass: 'Flick preserved' }
const magnitude = (dx: number, dy: number) => Math.hypot(dx, dy)
const TARGETS: CalibrationTarget[] = [
  { x: 50, y: 50, size: 58, label: 'Center check' },
  { x: 18, y: 24, size: 46, label: 'Target 1' }, { x: 82, y: 26, size: 40, label: 'Target 2' },
  { x: 26, y: 72, size: 34, label: 'Target 3' }, { x: 74, y: 68, size: 30, label: 'Target 4' },
  { x: 50, y: 20, size: 38, label: 'Target 5' }, { x: 50, y: 81, size: 42, label: 'Target 6' },
]
function samplePath(samples: number[]) {
  return samples.map((value, index) => `${index === 0 ? 'M' : 'L'} ${(index / Math.max(samples.length - 1, 1)) * 530} ${60 - value * 9}`).join(' ')
}

function App() {
  const [frames, setFrames] = useState<TelemetryFrame[]>(DEMO_FRAMES.slice(0, 1))
  const [running, setRunning] = useState(false)
  const [demoIndex, setDemoIndex] = useState(1)
  const [source, setSource] = useState<'demo' | 'daemon'>('demo')
  const [calibrationActive, setCalibrationActive] = useState(false)
  const [calibrationPhase, setCalibrationPhase] = useState<CalibrationPhase>('idle')
  const [profilePath, setProfilePath] = useState<string | null>(null)
  const [calibrationTrial, setCalibrationTrial] = useState(0)
  const [calibrationStartedAt, setCalibrationStartedAt] = useState(0)
  const [calibrationSamples, setCalibrationSamples] = useState<CalibrationSample[]>([])
  const [calibrationTrials, setCalibrationTrials] = useState<CalibrationTrial[]>([])
  const [bridgeStatus, setBridgeStatus] = useState('Bridge offline')
  const [stageElapsedSeconds, setStageElapsedSeconds] = useState(0)
  const [stageReportCount, setStageReportCount] = useState(0)
  const lastCalibrationSampleAt = useRef(0)
  const lastBridgeTimestamp = useRef(0)
  const completedTimedStage = useRef<CalibrationPhase | null>(null)

  useEffect(() => {
    const onTelemetry = (event: TelemetryEvent) => { setSource('daemon'); setRunning(true); setFrames((current) => [...current.slice(-59), event.detail]) }
    window.addEventListener('zerotremor:telemetry', onTelemetry)
    return () => window.removeEventListener('zerotremor:telemetry', onTelemetry)
  }, [])
  useEffect(() => {
    if (!running || source !== 'demo') return undefined
    const interval = window.setInterval(() => { setFrames((current) => [...current.slice(-59), DEMO_FRAMES[demoIndex]]); setDemoIndex((index) => (index + 1) % DEMO_FRAMES.length) }, 650)
    return () => window.clearInterval(interval)
  }, [demoIndex, running, source])
  useEffect(() => {
    if (!calibrationActive) return undefined
    const poll = async () => {
      try {
        const frame = await (await fetch('http://127.0.0.1:8765/v1/telemetry')).json() as Partial<TelemetryFrame> & { active?: boolean }
        if (frame.active && typeof frame.timestampUs === 'number' && frame.timestampUs !== lastBridgeTimestamp.current && typeof frame.rawDx === 'number' && typeof frame.rawDy === 'number' && typeof frame.correctedDx === 'number' && typeof frame.correctedDy === 'number' && typeof frame.mode === 'string') {
          lastBridgeTimestamp.current = frame.timestampUs
          setSource('daemon')
          setFrames((current) => [...current.slice(-59), frame as TelemetryFrame])
        }
      } catch { setBridgeStatus('Bridge connection lost') }
    }
    void poll()
    const interval = window.setInterval(() => { void poll() }, 40)
    return () => window.clearInterval(interval)
  }, [calibrationActive])
  useEffect(() => {
    if (!calibrationActive) return undefined
    const update = () => setStageElapsedSeconds(Math.floor((performance.now() - calibrationStartedAt) / 1000))
    update()
    const interval = window.setInterval(update, 250)
    return () => window.clearInterval(interval)
  }, [calibrationActive, calibrationStartedAt])
  useEffect(() => {
    const nextStage: Partial<Record<CalibrationPhase, { seconds: number; next: CalibrationPhase }>> = {
      still: { seconds: 10, next: 'slow' },
      slow: { seconds: 15, next: 'flick' },
      flick: { seconds: 10, next: 'deriving' },
    }
    const stage = nextStage[calibrationPhase]
    if (!calibrationActive || !stage || stageElapsedSeconds < stage.seconds || completedTimedStage.current === calibrationPhase) return
    completedTimedStage.current = calibrationPhase
    void finishStage(stage.next)
  }, [calibrationActive, calibrationPhase, stageElapsedSeconds])
  useEffect(() => {
    if (!calibrationActive) return undefined
    const pollStatus = async () => {
      try {
        const status = await (await fetch('http://127.0.0.1:8765/v1/status')).json() as { active?: boolean; reports?: number }
        if (!status.active) setBridgeStatus('Capture stopped unexpectedly')
        else if (typeof status.reports === 'number') setStageReportCount(status.reports)
      } catch { setBridgeStatus('Bridge connection lost') }
    }
    void pollStatus()
    const interval = window.setInterval(() => { void pollStatus() }, 250)
    return () => window.clearInterval(interval)
  }, [calibrationActive])

  const current = frames.at(-1) ?? DEMO_FRAMES[0]
  const counts = useMemo(() => frames.reduce<Record<FilterMode, number>>((total, frame) => ({ ...total, [frame.mode]: total[frame.mode] + 1 }), { Deadband: 0, Smooth: 0, PassThrough: 0, FlickBypass: 0 }), [frames])
  const rawTravel = frames.reduce((total, frame) => total + magnitude(frame.rawDx, frame.rawDy), 0)
  const correctedTravel = frames.reduce((total, frame) => total + magnitude(frame.correctedDx, frame.correctedDy), 0)
  const reduction = rawTravel === 0 ? 0 : Math.max(0, (1 - correctedTravel / rawTravel) * 100)
  const startDemo = () => { setSource('demo'); setFrames(DEMO_FRAMES.slice(0, 1)); setDemoIndex(1); setRunning(true) }
  const postBridge = async (path: string, data?: unknown) => {
    const response = await fetch(`http://127.0.0.1:8765${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: data === undefined ? undefined : JSON.stringify(data) })
    const result = await response.json() as { ok: boolean; error?: string }
    if (!result.ok) throw new Error(result.error ?? 'native bridge rejected the request')
  }
  const startStage = async (segment: 'still' | 'slow' | 'flick' | 'general', phase: CalibrationPhase) => {
    try { await postBridge('/v1/session/start', { segment }); setBridgeStatus(`Capturing ${segment}`) }
    catch (error) { setBridgeStatus(error instanceof Error ? error.message : 'Bridge offline'); return }
    setCalibrationStartedAt(performance.now())
    setStageElapsedSeconds(0)
    setStageReportCount(0)
    completedTimedStage.current = null
    lastCalibrationSampleAt.current = 0
    lastBridgeTimestamp.current = 0
    setCalibrationPhase(phase)
    setCalibrationActive(true)
  }
  const beginCalibration = () => {
    setCalibrationSamples([]); setCalibrationTrials([]); setCalibrationTrial(0); setProfilePath(null)
    void startStage('still', 'still')
  }
  const finishStage = async (next: CalibrationPhase) => {
    try { await postBridge('/v1/session/stop'); setBridgeStatus('Stage saved') }
    catch (error) { setBridgeStatus(error instanceof Error ? error.message : 'Bridge connection lost'); return }
    setCalibrationActive(false)
    // The bridge reads HID with a bounded timeout; allow it to observe stop,
    // flush the current JSONL writer, and release the device before the next stage.
    await new Promise((resolve) => window.setTimeout(resolve, 300))
    if (next === 'deriving') {
      setCalibrationPhase('deriving')
      try {
        const response = await fetch('http://127.0.0.1:8765/v1/calibration/derive', { method: 'POST' })
        const result = await response.json() as { ok: boolean; error?: string; profile_path?: string }
        if (!result.ok || !result.profile_path) throw new Error(result.error ?? 'profile derivation failed')
        setProfilePath(result.profile_path); setBridgeStatus('Profile saved — starting precision targets'); setCalibrationTrial(0); void startStage('general', 'targets')
      } catch (error) { setBridgeStatus(error instanceof Error ? error.message : 'Bridge connection lost'); setCalibrationPhase('flick') }
    } else if (next === 'slow') void startStage('slow', 'slow')
    else if (next === 'flick') void startStage('flick', 'flick')
    else setCalibrationPhase(next)
  }
  const startTargets = () => { setCalibrationTrial(0); void startStage('general', 'targets') }
  const captureCalibrationSample = (event: MouseEvent<HTMLDivElement>) => {
    if (!calibrationActive || calibrationPhase !== 'targets') return
    const now = performance.now()
    if (now - lastCalibrationSampleAt.current < 16) return
    lastCalibrationSampleAt.current = now
    const bounds = event.currentTarget.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]
    const sample = {
      tMs: now - calibrationStartedAt, trial: calibrationTrial,
      x: event.clientX - bounds.left, y: event.clientY - bounds.top, areaWidth: bounds.width, areaHeight: bounds.height,
      targetX: bounds.width * target.x / 100, targetY: bounds.height * target.y / 100, targetSize: target.size,
      telemetry: { rawDx: current.rawDx, rawDy: current.rawDy, correctedDx: current.correctedDx, correctedDy: current.correctedDy, mode: current.mode }, telemetrySource: source,
    }
    setCalibrationSamples((samples) => [...samples, sample])
    void postBridge('/v1/event', { kind: 'pointer_sample', ...sample }).catch(() => setBridgeStatus('Bridge connection lost'))
  }
  const recordTargetClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation()
    const bounds = event.currentTarget.parentElement!.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]
    const trial = { trial: calibrationTrial, target, clickX: event.clientX - bounds.left, clickY: event.clientY - bounds.top, elapsedMs: performance.now() - calibrationStartedAt, hit: true }
    setCalibrationTrials((trials) => [...trials, trial])
    void postBridge('/v1/event', { kind: 'target_click', ...trial }).catch(() => setBridgeStatus('Bridge connection lost'))
    if (calibrationTrial + 1 === TARGETS.length) { setCalibrationActive(false); setCalibrationPhase('complete'); void postBridge('/v1/session/stop').catch(() => setBridgeStatus('Bridge connection lost')) }
    else { setCalibrationTrial((trial) => trial + 1); setCalibrationStartedAt(performance.now()); lastCalibrationSampleAt.current = 0 }
  }
  const recordMiss = (event: MouseEvent<HTMLDivElement>) => {
    if (!calibrationActive || calibrationPhase !== 'targets' || event.target !== event.currentTarget) return
    const bounds = event.currentTarget.getBoundingClientRect()
    const target = TARGETS[calibrationTrial]
    const trial = { trial: calibrationTrial, target, clickX: event.clientX - bounds.left, clickY: event.clientY - bounds.top, elapsedMs: performance.now() - calibrationStartedAt, hit: false }
    setCalibrationTrials((trials) => [...trials, trial])
    void postBridge('/v1/event', { kind: 'target_miss', ...trial }).catch(() => setBridgeStatus('Bridge connection lost'))
  }
  const downloadCalibration = () => {
    const payload = { schemaVersion: 1, kind: 'target_centric_calibration', capturedAt: new Date().toISOString(), coordinateSpace: 'target_area_relative_pixels', viewport: { width: window.innerWidth, height: window.innerHeight, devicePixelRatio: window.devicePixelRatio }, device: 'demo-user / 1C4F:0048', trials: calibrationTrials, samples: calibrationSamples }
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }))
    const link = document.createElement('a'); link.href = url; link.download = 'zerotremor-target-calibration.json'; link.click(); URL.revokeObjectURL(url)
  }

  return <main className="console">
    <header className="topbar">
      <div className="brand"><span className="brand-mark">≈</span><div><p className="eyebrow">zeroTremor / safe validation</p><h1>Movement telemetry</h1></div></div>
      <div className="session-controls"><span className={`connection ${source}`}><i />{source === 'demo' ? 'Demo feed' : 'Daemon feed'}</span><button className="secondary" type="button" onClick={() => setRunning(false)} disabled={!running}>Pause</button><button className="secondary" type="button" onClick={beginCalibration}>{calibrationActive ? 'Restart calibration' : 'Start full calibration'}</button><button type="button" onClick={startDemo}>Run validation preview</button></div>
    </header>

    <section className="pipeline" aria-label="Movement processing pipeline">
      {[['Physical mouse', 'SIGMACHIP · 1C4F:0048', 'ready'], ['HID decoder', '4-byte relative report', 'ready'], ['Personalized filter', current.mode, 'active'], ['Virtual pointer', 'Corrected output only', 'ready']].map(([title, detail, state], index) => <div className="pipeline-piece" key={title}><div className={`node ${state}`}><span>{index + 1}</span></div><div><strong>{title}</strong><small>{detail}</small></div>{index < 3 && <div className="arrow">→</div>}</div>)}
    </section>

    <section className="panel calibration-panel">
      <div className="panel-heading"><div><p className="eyebrow">Complete personalized calibration</p><h2>Still → slow → flick → precision targets</h2></div><div className="calibration-actions"><span className="sample-count">{bridgeStatus} · {calibrationSamples.length} samples · {calibrationTrials.filter((trial) => trial.hit).length}/{TARGETS.length} hits</span>{calibrationTrials.length > 0 && <button className="secondary" type="button" onClick={downloadCalibration}>Download training JSON</button>}</div></div>
      <p className="calibration-copy">The first three stages create the deterministic personalized profile. The target stage then collects intent labels: destination, path shape, misses, timing, and filter telemetry.</p>
      {calibrationPhase === 'still' && <div className="calibration-instructions"><strong>1. Still hold · recording {stageElapsedSeconds}/10s · {stageReportCount} HID reports</strong><span>Capture is active. Leave the mouse untouched. A report count of zero is normal while it is truly still; the UI saves this stage and advances automatically at 10 seconds.</span><button type="button" onClick={() => void finishStage('slow')}>Save now</button></div>}
      {calibrationPhase === 'slow' && <div className="calibration-instructions"><strong>2. Slow intentional movement · recording {stageElapsedSeconds}/15s · {stageReportCount} HID reports</strong><span>Move deliberately in several directions. The report count should rise as the bridge receives input; this stage saves and advances automatically at 15 seconds.</span><button type="button" onClick={() => void finishStage('flick')}>Save now</button></div>}
      {calibrationPhase === 'flick' && <div className="calibration-instructions"><strong>3. Normal flicks · recording {stageElapsedSeconds}/10s · {stageReportCount} HID reports</strong><span>Make several normal fast flicks. This stage saves and creates your profile automatically at 10 seconds.</span><button type="button" onClick={() => void finishStage('deriving')}>Save now</button></div>}
      {calibrationPhase === 'deriving' && <div className="calibration-instructions"><strong>Creating profile…</strong><span>Deriving noise, slow-motion, smoothing, and flick thresholds locally.</span></div>}
      {calibrationPhase === 'targets' && !calibrationActive && <div className="calibration-instructions"><strong>Profile saved{profilePath ? `: ${profilePath}` : ''}</strong><span>The target exercise could not start. Confirm the bridge is running, then retry.</span><button type="button" onClick={startTargets}>Retry center check</button></div>}
      <div className={`target-area ${calibrationActive ? 'active' : ''}`} onMouseMove={captureCalibrationSample} onClick={recordMiss}>
        {calibrationPhase === 'idle' && <div className="calibration-instructions"><strong>Ready for a complete calibration.</strong><span>Start the native bridge, then begin with the still hold.</span><button type="button" onClick={beginCalibration}>Begin full calibration</button></div>}
        {calibrationActive && calibrationPhase === 'targets' && <><div className="target-progress">{TARGETS[calibrationTrial].label} · {calibrationTrial + 1}/{TARGETS.length}</div><button className="calibration-target" type="button" aria-label={TARGETS[calibrationTrial].label} onClick={recordTargetClick} style={{ left: `${TARGETS[calibrationTrial].x}%`, top: `${TARGETS[calibrationTrial].y}%`, width: TARGETS[calibrationTrial].size, height: TARGETS[calibrationTrial].size }}><span /></button></>}
        {calibrationPhase === 'complete' && <div className="calibration-instructions complete"><strong>Calibration capture complete.</strong><span>{profilePath ?? 'Profile saved'}; {calibrationSamples.length} browser-path samples and {calibrationTrials.filter((trial) => trial.hit).length} successful target labels are ready to export.</span><button type="button" onClick={beginCalibration}>Run again</button></div>}
      </div>
      <div className="calibration-footnote"><span>Browser coordinates: page-local x/y from pointer events</span><span>Raw HID coordinates: relative dx/dy, joined by daemon telemetry timestamp</span></div>
    </section>

    <section className="dashboard-grid">
      <article className="panel movement-panel"><div className="panel-heading"><div><p className="eyebrow">Live comparison</p><h2>Raw vs corrected movement</h2></div><span className={`mode-pill ${current.mode}`}>{modeLabel[current.mode]}</span></div>
        <div className="delta-pair"><div className="delta raw"><span>Raw input</span><strong>{current.rawDx > 0 ? '+' : ''}{current.rawDx}, {current.rawDy > 0 ? '+' : ''}{current.rawDy}</strong><small>dx, dy</small></div><div className="transfer">→</div><div className="delta corrected"><span>Virtual output</span><strong>{current.correctedDx > 0 ? '+' : ''}{current.correctedDx}, {current.correctedDy > 0 ? '+' : ''}{current.correctedDy}</strong><small>corrected dx, dy</small></div></div>
        <div className="signal-chart"><div className="chart-legend"><span><i className="raw-dot" />Raw dx</span><span><i className="corrected-dot" />Corrected dx</span></div><svg viewBox="0 0 530 120" preserveAspectRatio="none"><path className="baseline" d="M 0 60 L 530 60" /><path className="raw-line" d={samplePath(frames.slice(-18).map((frame) => frame.rawDx))} /><path className="corrected-line" d={samplePath(frames.slice(-18).map((frame) => frame.correctedDx))} /></svg></div>
      </article>
      <article className="panel profile-panel"><p className="eyebrow">Active profile</p><h2>demo-user.json</h2><dl><div><dt>Still noise p95</dt><dd>3.16</dd></div><div><dt>Deadband cap</dt><dd>0.75</dd></div><div><dt>Smoothing</dt><dd>35%</dd></div><div><dt>Flick threshold</dt><dd>94.01</dd></div></dl><p className="profile-note">Profile is matched to the demo mouse. Recalibrate after changing capture sessions.</p></article>
      <article className="panel metrics-panel"><div className="panel-heading"><div><p className="eyebrow">Validation result</p><h2>Travel preserved</h2></div><strong className="metric">{(100 - reduction).toFixed(0)}%</strong></div><div className="meter"><span style={{ width: `${100 - reduction}%` }} /></div><div className="metric-row"><span>Raw travel <b>{rawTravel.toFixed(1)}</b></span><span>Corrected <b>{correctedTravel.toFixed(1)}</b></span></div><p className="muted">{reduction.toFixed(1)}% movement removed by the filter in this feed.</p></article>
      <article className="panel modes-panel"><div className="panel-heading"><div><p className="eyebrow">Filter decisions</p><h2>Mode distribution</h2></div><span className="sample-count">{frames.length} samples</span></div><div className="mode-list">{(Object.keys(counts) as FilterMode[]).map((mode) => <div className="mode-row" key={mode}><span className={`mode-swatch ${mode}`} /><span>{modeLabel[mode]}</span><div className="mode-bar"><i style={{ width: `${(counts[mode] / Math.max(frames.length, 1)) * 100}%` }} /></div><b>{counts[mode]}</b></div>)}</div></article>
    </section>

    <section className="panel event-panel"><div className="panel-heading"><div><p className="eyebrow">Report stream</p><h2>Latest decoded reports</h2></div><span className="safety"><i />Recording replay — no live HID capture</span></div><div className="event-table"><div className="event-head"><span>Timestamp</span><span>Raw</span><span>Corrected</span><span>Decision</span><span>Pointer sink</span></div>{frames.slice(-6).reverse().map((frame) => <div className="event-row" key={frame.timestampUs}><span className="timestamp">{frame.timestampUs.toLocaleString()} µs</span><span>({frame.rawDx}, {frame.rawDy})</span><span className="corrected-value">({frame.correctedDx}, {frame.correctedDy})</span><span><i className={`table-dot ${frame.mode}`} />{modeLabel[frame.mode]}</span><span>{frame.correctedDx || frame.correctedDy ? 'emitted' : 'suppressed'}</span></div>)}</div></section>
  </main>
}

export default App
