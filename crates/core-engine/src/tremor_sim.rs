use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::PointerSample;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TremorConfig {
    /// Main synthetic oscillation frequency.
    pub frequency_hz: f32,

    /// Cursor-space displacement amplitude.
    pub amplitude_x: f32,
    pub amplitude_y: f32,

    /// Makes X/Y oscillation non-identical.
    pub y_phase_rad: f32,

    /// Slowly varies amplitude over time.
    pub amplitude_mod_hz: f32,
    pub amplitude_mod_depth: f32,
}

impl TremorConfig {
    /// Default tremor shape at the given frequency and X amplitude; Y moves at
    /// 75% of X so the two axes don't shake identically.
    pub fn new(frequency_hz: f32, amplitude: f32) -> Self {
        Self {
            frequency_hz,
            amplitude_x: amplitude,
            amplitude_y: amplitude * 0.75,
            ..Self::default()
        }
    }
}

impl Default for TremorConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 6.0,
            amplitude_x: 6.0,
            amplitude_y: 4.5,
            y_phase_rad: 1.2,
            amplitude_mod_hz: 0.35,
            amplitude_mod_depth: 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SimulatedPointerSample {
    /// Original intentional mouse movement.
    pub clean: PointerSample,

    /// Movement after synthetic tremor is added.
    pub observed: PointerSample,

    /// Amount of synthetic tremor added this sample.
    pub tremor_dx: f32,
    pub tremor_dy: f32,
}

#[derive(Debug)]
pub struct TremorSimulator {
    config: TremorConfig,
    start_us: Option<u64>,

    /// Tremor position at the previous sample.
    previous: (f32, f32),
}

impl TremorSimulator {
    pub fn new(config: TremorConfig) -> Self {
        let mut simulator = Self {
            config,
            start_us: None,
            previous: (0.0, 0.0),
        };

        // The first sample lands at t = 0, so starting from that position
        // prevents a fake jump on the first sample.
        simulator.previous = simulator.tremor_position(0.0);

        simulator
    }

    fn tremor_position(&self, time_seconds: f32) -> (f32, f32) {
        let modulation = 1.0
            + self.config.amplitude_mod_depth
                * (TAU * self.config.amplitude_mod_hz * time_seconds).sin();

        let phase = TAU * self.config.frequency_hz * time_seconds;

        let x = self.config.amplitude_x * modulation * phase.sin();

        let y = self.config.amplitude_y * modulation * (phase + self.config.y_phase_rad).sin();

        (x, y)
    }

    pub fn inject(&mut self, clean: PointerSample) -> SimulatedPointerSample {
        let start = *self.start_us.get_or_insert(clean.timestamp_us);

        let elapsed_us = clean.timestamp_us.saturating_sub(start);

        let time_seconds = elapsed_us as f32 / 1_000_000.0;

        let current = self.tremor_position(time_seconds);
        let (previous_x, previous_y) = std::mem::replace(&mut self.previous, current);

        // PointerSample contains relative movement, so convert
        // our oscillating POSITION into relative DELTAS.
        let tremor_dx = current.0 - previous_x;

        let tremor_dy = current.1 - previous_y;

        let observed = PointerSample {
            dx: clean.dx + tremor_dx,
            dy: clean.dy + tremor_dy,
            timestamp_us: clean.timestamp_us,
        };

        SimulatedPointerSample {
            clean,
            observed,
            tremor_dx,
            tremor_dy,
        }
    }
}
