#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    live::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The live cursor demo is currently implemented for macOS.");
}

#[cfg(target_os = "macos")]
mod live {
    use std::{
        env,
        error::Error,
        fs::{create_dir_all, File},
        io::{self, BufWriter, Write},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };

    use core_engine::{
        PointerSample, PointerSink, SimulatedPointerSample, TremorConfig, TremorSimulator,
    };
    use hid_capture::{open_device, DeviceSelection, DeviceSelector, MouseReport, OpenedDevice};
    use os_virtual_input::MacOsPointerSink;

    // 125 Hz simulator
    const TICK: Duration = Duration::from_micros(8_000);

    struct Args {
        frequency_hz: f32,
        amplitude: f32,
        output_path: String,
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let args = parse_args();
        let mut writer = create_csv(&args.output_path)?;

        let mut simulator =
            TremorSimulator::new(TremorConfig::new(args.frequency_hz, args.amplitude));

        let opened = open_demo_mouse(&args)?;
        let running = stop_on_ctrl_c()?;
        let mut sink = MacOsPointerSink::new()?;

        let start = Instant::now();
        let mut next_tick = Instant::now();

        while running.load(Ordering::SeqCst) {
            next_tick += TICK;

            let (dx, dy) = drain_reports(&opened, &mut sink)?;
            let timestamp_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
            let simulated = simulator.inject(PointerSample {
                dx,
                dy,
                timestamp_us,
            });

            write_row(&mut writer, &args, &simulated)?;
            sink.emit_relative(simulated.observed.dx, simulated.observed.dy)?;

            sleep_until(&mut next_tick);
        }

        writer.flush()?;

        println!();
        println!("Simulation stopped.");
        println!("Mouse released.");
        println!("Saved: {}", args.output_path);

        Ok(())
    }

    fn parse_args() -> Args {
        let args: Vec<String> = env::args().collect();
        let number_or = |index: usize, default: f32| {
            args.get(index)
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };

        Args {
            frequency_hz: number_or(1, 6.0),
            amplitude: number_or(2, 6.0),
            output_path: args
                .get(3)
                .cloned()
                .unwrap_or_else(|| "recordings/synthetic_tremor.csv".to_string()),
        }
    }

    fn create_csv(path: &str) -> io::Result<BufWriter<File>> {
        create_dir_all("recordings")?;

        let mut writer = BufWriter::new(File::create(path)?);

        writeln!(
            writer,
            "timestamp_us,frequency_hz,amplitude_px,\
clean_dx,clean_dy,tremor_dx,tremor_dy,\
observed_dx,observed_dy"
        )?;

        Ok(writer)
    }

    fn open_demo_mouse(args: &Args) -> Result<OpenedDevice, Box<dyn Error>> {
        println!("Opening demo mouse 1C4F:0048...");

        let opened = open_device(DeviceSelection::VidPid(DeviceSelector {
            vendor_id: 0x1c4f,
            product_id: 0x0048,
        }))?;

        println!("Mouse seized.");
        println!(
            "Synthetic tremor: {:.1} Hz, {:.1}px amplitude",
            args.frequency_hz, args.amplitude
        );
        println!("Saving data to: {}", args.output_path);
        println!("Press Ctrl+C to stop.");

        Ok(opened)
    }

    fn stop_on_ctrl_c() -> Result<Arc<AtomicBool>, ctrlc::Error> {
        let running = Arc::new(AtomicBool::new(true));
        let handler_running = Arc::clone(&running);

        ctrlc::set_handler(move || handler_running.store(false, Ordering::SeqCst))?;

        Ok(running)
    }

    /// Drains queued HID reports and returns the intentional movement collected
    /// from the physical mouse during this 8 ms window. The mouse is seized, so
    /// clicks and scrolling are forwarded to the sink here too.
    fn drain_reports(
        opened: &OpenedDevice,
        sink: &mut MacOsPointerSink,
    ) -> Result<(f32, f32), Box<dyn Error>> {
        let (mut dx, mut dy, mut wheel) = (0.0_f32, 0.0_f32, 0_i32);

        for _ in 0..32 {
            let Some(raw) = opened.read_raw(0)? else {
                break;
            };

            let Some(report) = MouseReport::decode(&raw.bytes) else {
                continue;
            };

            dx += report.dx;
            dy += report.dy;
            wheel += i32::from(report.wheel);

            // Per report, so a press and release inside one tick both land.
            sink.set_buttons(report.buttons)?;
        }

        sink.scroll(wheel)?;

        Ok((dx, dy))
    }

    fn write_row(
        writer: &mut impl Write,
        args: &Args,
        simulated: &SimulatedPointerSample,
    ) -> io::Result<()> {
        writeln!(
            writer,
            "{},{:.3},{:.3},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5}",
            simulated.clean.timestamp_us,
            args.frequency_hz,
            args.amplitude,
            simulated.clean.dx,
            simulated.clean.dy,
            simulated.tremor_dx,
            simulated.tremor_dy,
            simulated.observed.dx,
            simulated.observed.dy,
        )
    }

    fn sleep_until(next_tick: &mut Instant) {
        let now = Instant::now();

        if *next_tick > now {
            thread::sleep(*next_tick - now);
        } else {
            // Don't accumulate timing lag.
            *next_tick = now;
        }
    }
}
