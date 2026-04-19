//! Async Embassy entry point.
//!
//! [`run()`] wires the full keyboard pipeline: matrix scan → debounce →
//! [`KeyboardProcessor`] → [`AsyncReporter`]. Call it from your board crate's
//! Embassy entry point and never return.
//!
//! # Example
//!
//! ```rust,ignore
//! #[embassy_executor::main]
//! async fn main(_spawner: Spawner) {
//!     let scanner  = MyMatrixScanner::new(/* GPIO pins */);
//!     let debouncer = AsymEagerDefer::new(5);
//!     let processor = KeyboardProcessor::new(MY_LAYERS, TapHoldConfig::default());
//!     let reporter  = MyUsbReporter::new(/* USB driver */);
//!     tecla::task::run(scanner, debouncer, processor, reporter).await;
//! }
//! ```
//!
//! # Tap-hold timing
//!
//! When [`KeyboardProcessor::next_deadline_ms`] returns a deadline the scan is
//! raced against an [`embassy_time`] timer via [`with_timeout`]. On timeout,
//! [`KeyboardProcessor::tick`] is called to resolve the pending tap-hold.

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{with_timeout, Duration, Instant};
use heapless::Vec;

use crate::action::Action;
use crate::devices::encoder::{AsyncEncoderScanner, EncoderMapping};
use crate::devices::mouse::pointing::AsyncPointingScanner;
use crate::keyboard::KeyboardProcessor;
use crate::matrix::debounce::Debouncer;
use crate::matrix::{AsyncMatrixScanner, MatrixState};
use crate::pipeline::event::{EncoderDirection, KeyEvent};
use crate::pipeline::hid::AsyncReporter;
use crate::pipeline::report::{ConsumerReport, HidReport, KeyboardReport, MouseReport};
use crate::telemetry::encoder::EncoderTelemetry;
use crate::telemetry::pointing::PointingTelemetry;
use crate::transport::SplitTransport;

/// A channel for routing [`KeyEvent`]s from [`matrix_task`] to [`keyboard_task`].
///
/// `M` is the mutex type — use `embassy_sync::blocking_mutex::raw::NoopRawMutex`
/// on single-core targets (RP2040, most STM32) and `CriticalSectionRawMutex`
/// when tasks run on different cores or interrupt priorities.
///
/// `N` is the channel capacity. 16 is a reasonable default: it absorbs a full
/// key roll-over burst while the keyboard task is processing a previous event,
/// but stays well within typical RAM budgets.
pub type EventChannel<M, const N: usize> = Channel<M, KeyEvent, N>;

/// A channel for routing [`HidReport`]s from [`keyboard_task`] to [`usb_task`].
///
/// `M` is the mutex type (see [`EventChannel`] for guidance).
///
/// `N` is the channel capacity. 8 is a reasonable default: consecutive reports
/// are coalesced inside [`usb_task`] so the queue depth rarely exceeds 2–3
/// in practice. Increase it if your USB polling rate is much lower than your
/// scan rate.
pub type ReportChannel<M, const N: usize> = Channel<M, HidReport, N>;

/// A channel for routing [`TelemetryFrame`](crate::telemetry::transport::TelemetryFrame)s
/// from the keyboard task to [`telemetry_task`].
///
/// Capacity of 2 is sufficient — the keyboard task sends at most one frame
/// per interval, and the telemetry writer drains it between intervals.
pub type TelemetryChannel<M, const N: usize> =
    Channel<M, crate::telemetry::transport::TelemetryFrame, N>;

/// Scans the key matrix, debounces events, and forwards [`KeyEvent`]s to `sender`.
///
/// Intended to run as an independent Embassy task so the matrix scan loop is
/// decoupled from HID report generation. Pair with [`keyboard_task`] and
/// [`usb_task`] to form the full multi-task pipeline.
///
/// This function never returns.
pub async fn matrix_task<M, S, Db, const ROWS: usize, const COLS: usize, const N: usize>(
    mut scanner: S,
    mut debouncer: Db,
    sender: Sender<'_, M, KeyEvent, N>,
) -> !
where
    M: RawMutex,
    S: AsyncMatrixScanner<ROWS, COLS>,
    Db: Debouncer<ROWS, COLS>,
{
    let mut prev: MatrixState<ROWS, COLS> = MatrixState::new();
    loop {
        let Ok(raw) = scanner.scan().await else {
            continue;
        };
        let now_ms = Instant::now().as_millis();
        let Some(stable) = debouncer.process(&raw, now_ms) else {
            continue;
        };
        for (pos, pressed) in stable.iter_pos() {
            let was_pressed = prev.get_pos(pos).unwrap_or(false);
            if pressed != was_pressed {
                let event = if pressed {
                    KeyEvent::press(pos, now_ms)
                } else {
                    KeyEvent::release(pos, now_ms)
                };
                sender.send(event).await;
            }
        }
        prev = stable;
    }
}

async fn receive_and_process<
    M: RawMutex,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext: Copy + PartialEq,
    const EN: usize,
    const RN: usize,
>(
    event_rx: &Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: &Sender<'_, M, HidReport, RN>,
    max_wait_ms: Option<u64>,
) {
    let now_ms = Instant::now().as_millis();
    let deadline_ms = processor.next_deadline_ms();
    let tap_hold_wait = deadline_ms.map(|d| d.saturating_sub(now_ms));
    let wait_ms = match (tap_hold_wait, max_wait_ms) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };

    let event_opt = match wait_ms {
        Some(ms) => match with_timeout(Duration::from_millis(ms), event_rx.receive()).await {
            Err(_timeout) => {
                let tick_ms = Instant::now().as_millis();
                if deadline_ms.is_some_and(|d| tick_ms >= d) {
                    processor.tick(tick_ms, &mut |r| {
                        let _ = report_tx.try_send(r);
                    });
                }
                None
            }
            Ok(ev) => Some(ev),
        },
        None => Some(event_rx.receive().await),
    };

    if let Some(event) = event_opt {
        let ev_ms = Instant::now().as_millis();
        processor.process_event(event, ev_ms, &mut |r| {
            let _ = report_tx.try_send(r);
        });
    }
}

/// Receives [`KeyEvent`]s, processes them through the keyboard pipeline, and
/// forwards [`HidReport`]s to `report_tx`.
///
/// When a tap-hold is pending, the channel receive is raced against the
/// deadline timer via [`with_timeout`]. On timeout, [`KeyboardProcessor::tick`]
/// resolves the pending tap-hold.
///
/// This function never returns.
pub async fn keyboard_task<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    mut processor: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, &mut processor, &report_tx, None).await;
    }
}

/// Like [`keyboard_task`] but also forwards BLE commands to a
/// [`BleCommandChannel`](crate::transport::ble::BleCommandChannel).
///
/// When the processor yields a BLE action (`BleAdvertise`, `BleDisconnect`,
/// `BleHostSwitch`), the corresponding [`BleCommand`](crate::transport::ble::BleCommand)
/// is sent to `ble_tx`.
///
/// This function never returns.
pub async fn keyboard_task_ble<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const BN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    mut processor: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    ble_tx: Sender<'_, M, crate::transport::ble::BleCommand, BN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, &mut processor, &report_tx, None).await;
        if let Some(cmd) = processor.pending_ble_command() {
            let _ = ble_tx.try_send(cmd);
        }
    }
}

/// Like [`keyboard_task`] but periodically collects telemetry snapshots and
/// sends them to `telem_tx`.
///
/// The telemetry interval is 250ms. If the processor has telemetry enabled
/// (via [`KeyboardProcessor::with_telemetry`]), a [`TelemetryFrame`] is
/// collected and sent each interval. If telemetry is not enabled, no frames
/// are sent.
///
/// This function never returns.
pub async fn keyboard_task_telemetry<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const TN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    mut processor: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    telem_tx: Sender<'_, M, crate::telemetry::transport::TelemetryFrame, TN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    const TELEM_INTERVAL_MS: u64 = 250;
    let mut last_telem_ms = Instant::now().as_millis();

    loop {
        let now_ms = Instant::now().as_millis();
        let telem_remaining =
            TELEM_INTERVAL_MS.saturating_sub(now_ms.saturating_sub(last_telem_ms));
        receive_and_process(&event_rx, &mut processor, &report_tx, Some(telem_remaining)).await;

        let now = Instant::now().as_millis();
        if now.saturating_sub(last_telem_ms) >= TELEM_INTERVAL_MS {
            if let Some(t) = processor.telemetry_mut() {
                let frame = crate::telemetry::transport::collect_frame(t);
                let _ = telem_tx.try_send(frame);
            }
            last_telem_ms = now;
        }
    }
}

/// Receives [`TelemetryFrame`](crate::telemetry::transport::TelemetryFrame)s
/// and writes serialized bytes to an [`AsyncTelemetryWriter`].
///
/// Pair with [`keyboard_task_telemetry`] to form the telemetry pipeline.
///
/// This function never returns.
pub async fn telemetry_task<M, W, const N: usize>(
    receiver: Receiver<'_, M, crate::telemetry::transport::TelemetryFrame, N>,
    mut writer: W,
) -> !
where
    M: RawMutex,
    W: crate::telemetry::transport::AsyncTelemetryWriter,
{
    let mut buf = heapless::Vec::<u8, 128>::new();
    loop {
        let frame = receiver.receive().await;
        frame.serialize(&mut buf);
        let _ = writer.write(&buf).await;
    }
}

/// Receives [`HidReport`]s and sends them to the USB reporter.
///
/// Consecutive identical reports are coalesced — the reporter is only called
/// when the report content actually changes. This eliminates redundant USB
/// transfers within a scan cycle.
///
/// This function never returns.
pub async fn usb_task<M, R, const N: usize>(
    receiver: Receiver<'_, M, HidReport, N>,
    mut reporter: R,
) -> !
where
    M: RawMutex,
    R: AsyncReporter,
{
    let mut last: Option<HidReport> = None;
    loop {
        let report = receiver.receive().await;
        if Some(report) != last {
            let _ = reporter.send(&report).await;
            last = Some(report);
        }
    }
}

/// Runs the full keyboard firmware pipeline.
///
/// Loops forever:
/// 1. Calls `scanner.scan()` for the raw matrix state (async — yields until ready).
/// 2. Passes the result through `debouncer`; skips if still settling.
/// 3. Diffs the stable state against the previous scan to produce [`KeyEvent`]s.
/// 4. Feeds events to `processor`, collecting [`HidReport`]s.
/// 5. Sends each report via `reporter`.
///
/// When a tap-hold is pending, the scan is raced against the deadline timer.
/// A timeout calls [`KeyboardProcessor::tick`] before resuming the scan loop.
///
/// This function never returns.
pub async fn run<
    S,
    Db,
    R,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
>(
    mut scanner: S,
    mut debouncer: Db,
    mut processor: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    mut reporter: R,
) -> !
where
    S: AsyncMatrixScanner<ROWS, COLS>,
    Db: Debouncer<ROWS, COLS>,
    R: AsyncReporter,
    Ext: Copy + PartialEq,
{
    let mut prev: MatrixState<ROWS, COLS> = MatrixState::new();
    loop {
        let mut reports: Vec<HidReport, 16> = Vec::new();

        let raw_opt = if let Some(deadline_ms) = processor.next_deadline_ms() {
            let now_ms = Instant::now().as_millis();
            let remaining = Duration::from_millis(deadline_ms.saturating_sub(now_ms));
            match with_timeout(remaining, scanner.scan()).await {
                Err(_timeout) => {
                    let tick_ms = Instant::now().as_millis();
                    processor.tick(tick_ms, &mut |r| {
                        let _ = reports.push(r);
                    });
                    None
                }
                Ok(Ok(raw)) => Some(raw),
                Ok(Err(_scan_err)) => None,
            }
        } else {
            scanner.scan().await.ok()
        };

        if let Some(raw) = raw_opt {
            let now_ms = Instant::now().as_millis();
            process_scan(
                &raw,
                &mut prev,
                now_ms,
                &mut debouncer,
                &mut processor,
                &mut reports,
            );
        }

        for report in &reports {
            let _ = reporter.send(report).await;
        }
    }
}

fn process_scan<
    Db,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
>(
    raw: &MatrixState<ROWS, COLS>,
    prev: &mut MatrixState<ROWS, COLS>,
    now_ms: u64,
    debouncer: &mut Db,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    reports: &mut Vec<HidReport, 16>,
) where
    Db: Debouncer<ROWS, COLS>,
    Ext: Copy + PartialEq,
{
    if let Some(stable) = debouncer.process(raw, now_ms) {
        for (pos, pressed) in stable.iter_pos() {
            let was_pressed = prev.get_pos(pos).unwrap_or(false);
            if pressed != was_pressed {
                let event = if pressed {
                    KeyEvent::press(pos, now_ms)
                } else {
                    KeyEvent::release(pos, now_ms)
                };
                processor.process_event(event, now_ms, &mut |r| {
                    let _ = reports.push(r);
                });
            }
        }
        *prev = stable;
    }
}

/// Receives [`KeyEvent`]s from a [`SplitTransport`] and forwards them into
/// the event channel.
///
/// Run this on the **primary half** of a split keyboard. The secondary half
/// sends debounced events over the transport; this task bridges them into the
/// same [`EventChannel`] that the local [`matrix_task`] feeds.
///
/// This function never returns.
pub async fn remote_event_task<M, T, const N: usize>(
    mut transport: T,
    sender: Sender<'_, M, KeyEvent, N>,
) -> !
where
    M: RawMutex,
    T: SplitTransport,
{
    loop {
        let Ok(event) = transport.recv().await else {
            continue;
        };
        sender.send(event).await;
    }
}

/// Scans a rotary encoder and sends synthetic [`KeyEvent`]s into the event
/// channel so that encoder rotations flow through the full keyboard pipeline
/// (layer resolution, modifiers, etc.).
///
/// Each encoder tick produces a press event immediately followed by a release
/// event at a virtual [`KeyPos`] computed by [`encoder_key_pos`]. The user's
/// keymap must include actions at those positions (see [`encoder_key_pos`]
/// for the indexing scheme).
///
/// `matrix_rows` is the number of physical matrix rows — used to place encoder
/// positions in a virtual row beyond the matrix.
///
/// This function never returns.
///
/// [`encoder_key_pos`]: crate::devices::encoder::encoder_key_pos
pub async fn encoder_task<M, S, const N: usize>(
    scanner: S,
    matrix_rows: u8,
    event_tx: Sender<'_, M, KeyEvent, N>,
) -> !
where
    M: RawMutex,
    S: AsyncEncoderScanner,
{
    encoder_task_with_telemetry::<M, S, N, 0>(scanner, matrix_rows, event_tx, None).await
}

/// Like [`encoder_task`] but records per-encoder rotation and reversal metrics.
pub async fn encoder_task_with_telemetry<M, S, const N: usize, const EN: usize>(
    mut scanner: S,
    matrix_rows: u8,
    event_tx: Sender<'_, M, KeyEvent, N>,
    mut telemetry: Option<&mut EncoderTelemetry<EN>>,
) -> !
where
    M: RawMutex,
    S: AsyncEncoderScanner,
{
    loop {
        let event = scanner.scan().await;
        if let Some(ref mut t) = telemetry {
            match event.direction {
                EncoderDirection::Clockwise => t.record_cw(event.encoder_id),
                EncoderDirection::CounterClockwise => t.record_ccw(event.encoder_id),
            }
        }
        let pos = crate::devices::encoder::encoder_key_pos(
            event.encoder_id,
            event.direction,
            matrix_rows,
        );
        let ts = event.timestamp_ms;
        event_tx.send(KeyEvent::press(pos, ts)).await;
        event_tx.send(KeyEvent::release(pos, ts)).await;
    }
}

/// Scans a rotary encoder and emits [`HidReport`]s directly for each tick,
/// bypassing the keyboard processor and layer system.
///
/// Each encoder tick produces a press report followed by a release report,
/// mapping the rotation direction to the corresponding action in `mappings`.
///
/// Prefer [`encoder_task`] for layer-aware encoder integration. Use this
/// variant only when you need a lightweight path that skips the full pipeline.
///
/// This function never returns.
pub async fn encoder_direct_task<M, S, Ext, const N: usize>(
    mut scanner: S,
    mappings: &[EncoderMapping<Ext>],
    report_tx: Sender<'_, M, HidReport, N>,
) -> !
where
    M: RawMutex,
    S: AsyncEncoderScanner,
    Ext: Copy + PartialEq,
{
    loop {
        let event = scanner.scan().await;
        for mapping in mappings {
            if mapping.encoder_id != event.encoder_id {
                continue;
            }
            let action = match event.direction {
                EncoderDirection::Clockwise => mapping.cw,
                EncoderDirection::CounterClockwise => mapping.ccw,
            };
            if let Some(report) = encoder_action_to_report(action) {
                report_tx.send(report).await;
                if let Some(release) = encoder_release_report(action) {
                    report_tx.send(release).await;
                }
            }
            break;
        }
    }
}

fn encoder_action_to_report<Ext: Copy + PartialEq>(action: Action<Ext>) -> Option<HidReport> {
    match action {
        Action::Key(kc) => {
            let mut report = KeyboardReport::new();
            report.keycodes[0] = kc.usage_id();
            Some(HidReport::Keyboard(report))
        }
        Action::Consumer(cc) => Some(HidReport::Consumer(ConsumerReport { keycode: Some(cc) })),
        Action::Mouse(ma) => {
            use crate::action::MouseAction;
            match ma {
                MouseAction::Move { dx, dy } => Some(HidReport::Mouse(MouseReport {
                    buttons: 0,
                    dx,
                    dy,
                    scroll: 0,
                })),
                MouseAction::Scroll(amount) => Some(HidReport::Mouse(MouseReport {
                    buttons: 0,
                    dx: 0,
                    dy: 0,
                    scroll: amount,
                })),
                MouseAction::Click(_) => None,
            }
        }
        _ => None,
    }
}

fn encoder_release_report<Ext: Copy + PartialEq>(action: Action<Ext>) -> Option<HidReport> {
    match action {
        Action::Key(_) => Some(HidReport::Keyboard(KeyboardReport::new())),
        Action::Consumer(_) => Some(HidReport::Consumer(ConsumerReport::new())),
        _ => None,
    }
}

/// Converts a [`PointingEvent`] into a [`HidReport::Mouse`].
///
/// Movement values are clamped to the `i8` range.
#[must_use]
#[expect(clippy::as_conversions, reason = "clamped values cast to i8")]
pub fn pointing_event_to_report(event: &crate::pipeline::event::PointingEvent) -> HidReport {
    #[expect(clippy::cast_possible_truncation, reason = "clamped to i8 range")]
    let dx = event.dx.clamp(-128, 127) as i8;
    #[expect(clippy::cast_possible_truncation, reason = "clamped to i8 range")]
    let dy = event.dy.clamp(-128, 127) as i8;
    HidReport::Mouse(MouseReport {
        buttons: event.buttons,
        dx,
        dy,
        scroll: event.scroll,
    })
}

/// Scans a pointing device and emits [`HidReport::Mouse`] reports.
///
/// Movement values are clamped to the `i8` range before being sent.
///
/// This function never returns.
pub async fn pointing_task<M, S, const N: usize>(
    scanner: S,
    report_tx: Sender<'_, M, HidReport, N>,
) -> !
where
    M: RawMutex,
    S: AsyncPointingScanner,
{
    pointing_task_with_telemetry(scanner, report_tx, None).await
}

/// Like [`pointing_task`] but records movement, click, and scroll metrics.
pub async fn pointing_task_with_telemetry<M, S, const N: usize>(
    mut scanner: S,
    report_tx: Sender<'_, M, HidReport, N>,
    mut telemetry: Option<&mut PointingTelemetry>,
) -> !
where
    M: RawMutex,
    S: AsyncPointingScanner,
{
    let mut last_moving = false;
    loop {
        let event = scanner.scan().await;
        let moving = event.dx != 0 || event.dy != 0;
        if let Some(ref mut t) = telemetry {
            if moving {
                #[expect(clippy::cast_possible_truncation, reason = "clamped to i8 range")]
                let dx = event.dx.clamp(-128, 127) as i8;
                #[expect(clippy::cast_possible_truncation, reason = "clamped to i8 range")]
                let dy = event.dy.clamp(-128, 127) as i8;
                t.record_movement(dx, dy);
            }
            if event.buttons != 0 {
                t.record_click(moving || last_moving);
            }
            if event.scroll != 0 {
                t.record_scroll();
            }
        }
        last_moving = moving;
        report_tx.send(pointing_event_to_report(&event)).await;
    }
}

/// Scans the matrix, debounces, and sends [`KeyEvent`]s over a
/// [`SplitTransport`].
///
/// Run this on the **secondary half** of a split keyboard — the half without
/// USB. Events are sent to the primary half which merges them with its own
/// matrix events and handles HID reporting.
///
/// This function never returns.
pub async fn secondary_task<T, S, Db, const ROWS: usize, const COLS: usize>(
    mut scanner: S,
    mut debouncer: Db,
    mut transport: T,
) -> !
where
    T: SplitTransport,
    S: AsyncMatrixScanner<ROWS, COLS>,
    Db: Debouncer<ROWS, COLS>,
{
    let mut prev: MatrixState<ROWS, COLS> = MatrixState::new();
    loop {
        let Ok(raw) = scanner.scan().await else {
            continue;
        };
        let now_ms = Instant::now().as_millis();
        let Some(stable) = debouncer.process(&raw, now_ms) else {
            continue;
        };
        for (pos, pressed) in stable.iter_pos() {
            let was_pressed = prev.get_pos(pos).unwrap_or(false);
            if pressed != was_pressed {
                let event = if pressed {
                    KeyEvent::press(pos, now_ms)
                } else {
                    KeyEvent::release(pos, now_ms)
                };
                let _ = transport.send(event).await;
            }
        }
        prev = stable;
    }
}

/// Ticks the RGB animation engine and writes the LED buffer to hardware.
///
/// This function never returns.
#[expect(
    clippy::cast_possible_truncation,
    reason = "elapsed ms wraps at u32::MAX which is ~49 days — acceptable for animation timing"
)]
#[expect(
    clippy::as_conversions,
    reason = "elapsed ms truncated to u32 for animation timing"
)]
pub async fn rgb_task<D: crate::rgb::LedDriver, A: crate::rgb::Animation, const LEDS: usize>(
    controller: &mut crate::rgb::RgbController<LEDS>,
    driver: &mut D,
    animation: &mut A,
    tick_ms: u32,
) -> ! {
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed().as_millis() as u32;
        controller.tick(elapsed, animation);
        let _ = driver.write(controller.buffer()).await;
        embassy_time::Timer::after(Duration::from_millis(u64::from(tick_ms))).await;
    }
}

/// Receives [`AudioCommand`](crate::audio::AudioCommand)s and plays them via the buzzer driver.
///
/// This function never returns.
pub async fn audio_task<M, D, const N: usize>(
    receiver: embassy_sync::channel::Receiver<'_, M, crate::audio::AudioCommand, N>,
    mut driver: D,
) -> !
where
    M: RawMutex,
    D: crate::audio::BuzzerDriver,
{
    loop {
        let cmd = receiver.receive().await;
        match cmd {
            crate::audio::AudioCommand::PlayTone(tone) => {
                let _ = driver.play_tone(tone.frequency_hz, tone.duration_ms).await;
            }
            crate::audio::AudioCommand::PlayMelody(melody) => {
                for tone in melody {
                    if tone.frequency_hz == 0 {
                        embassy_time::Timer::after(Duration::from_millis(u64::from(
                            tone.duration_ms,
                        )))
                        .await;
                    } else {
                        let _ = driver.play_tone(tone.frequency_hz, tone.duration_ms).await;
                    }
                }
            }
            crate::audio::AudioCommand::Stop => {
                let _ = driver.stop().await;
            }
            crate::audio::AudioCommand::Toggle => {}
        }
    }
}

/// Receives [`HapticEffect`](crate::haptic::HapticEffect)s and plays them via the haptic driver.
///
/// This function never returns.
pub async fn haptic_task<M, D, const N: usize>(
    receiver: embassy_sync::channel::Receiver<'_, M, crate::haptic::HapticEffect, N>,
    mut driver: D,
) -> !
where
    M: RawMutex,
    D: crate::haptic::HapticDriver,
{
    loop {
        let effect = receiver.receive().await;
        let _ = driver.play(&effect).await;
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::as_conversions, reason = "test code")]
#[expect(
    clippy::similar_names,
    reason = "enc_cw_pos and enc_ccw_pos are distinct directions"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "test code with known-valid indices"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test code with small constants"
)]
mod tests {
    use super::*;
    use crate::action::{Action, KeyCode};
    use crate::devices::keyboard::tapping::TapHoldConfig;
    use crate::matrix::debounce::Debouncer;
    use crate::matrix::MatrixState;
    use crate::pipeline::event::KeyPos;
    use crate::pipeline::keymap::{Layer, LayerStack};
    use crate::pipeline::report::HidReport;

    struct PassthroughDebouncer;

    impl<const ROWS: usize, const COLS: usize> Debouncer<ROWS, COLS> for PassthroughDebouncer {
        fn process(
            &mut self,
            raw: &MatrixState<ROWS, COLS>,
            _now_ms: u64,
        ) -> Option<MatrixState<ROWS, COLS>> {
            Some(raw.clone())
        }
    }

    struct RejectDebouncer;

    impl<const ROWS: usize, const COLS: usize> Debouncer<ROWS, COLS> for RejectDebouncer {
        fn process(
            &mut self,
            _raw: &MatrixState<ROWS, COLS>,
            _now_ms: u64,
        ) -> Option<MatrixState<ROWS, COLS>> {
            None
        }
    }

    const ROWS: usize = 2;
    const COLS: usize = 3;
    const LAYERS: usize = 1;
    const KEYS: usize = 6;

    fn make_processor(base: &[Action]) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
        let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
        let _ = layers.push(Layer::new(base));
        KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
    }

    fn key_a_layout() -> [Action; 6] {
        [
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]
    }

    fn keycodes(report: &HidReport) -> [u8; 6] {
        match report {
            HidReport::Keyboard(r) => r.keycodes,
            HidReport::Consumer(_)
            | HidReport::NkroKeyboard(_)
            | HidReport::Mouse(_)
            | HidReport::Midi(_)
            | HidReport::Gamepad(_) => [0; 6],
        }
    }

    #[test]
    fn key_press_generates_report_with_keycode() {
        let mut proc = make_processor(&key_a_layout());
        let mut prev = MatrixState::<ROWS, COLS>::new();
        let mut raw = MatrixState::<ROWS, COLS>::new();
        raw.set(0, 0, true);
        let mut reports: Vec<HidReport, 16> = Vec::new();

        process_scan(
            &raw,
            &mut prev,
            0,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut reports,
        );

        assert_eq!(reports.len(), 1);
        assert!(keycodes(reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    #[test]
    fn key_release_clears_keycode_from_report() {
        let mut proc = make_processor(&key_a_layout());
        let mut prev = MatrixState::<ROWS, COLS>::new();

        let mut pressed = MatrixState::<ROWS, COLS>::new();
        pressed.set(0, 0, true);
        process_scan(
            &pressed,
            &mut prev,
            0,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut Vec::new(),
        );

        let released = MatrixState::<ROWS, COLS>::new();
        let mut reports: Vec<HidReport, 16> = Vec::new();
        process_scan(
            &released,
            &mut prev,
            10,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut reports,
        );

        assert_eq!(reports.len(), 1);
        assert!(!keycodes(reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    #[test]
    fn unchanged_state_generates_no_reports() {
        let mut proc = make_processor(&key_a_layout());
        let mut prev = MatrixState::<ROWS, COLS>::new();
        let raw = MatrixState::<ROWS, COLS>::new();
        let mut reports: Vec<HidReport, 16> = Vec::new();

        process_scan(
            &raw,
            &mut prev,
            0,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut reports,
        );
        process_scan(
            &raw,
            &mut prev,
            1,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut reports,
        );

        assert!(reports.is_empty());
    }

    #[test]
    fn debouncer_returning_none_generates_no_reports() {
        let mut proc = make_processor(&key_a_layout());
        let mut prev = MatrixState::<ROWS, COLS>::new();
        let mut raw = MatrixState::<ROWS, COLS>::new();
        raw.set(0, 0, true);
        let mut reports: Vec<HidReport, 16> = Vec::new();

        process_scan(
            &raw,
            &mut prev,
            0,
            &mut RejectDebouncer,
            &mut proc,
            &mut reports,
        );

        assert!(reports.is_empty());
    }

    #[test]
    fn prev_state_is_updated_after_stable_scan() {
        let mut proc = make_processor(&key_a_layout());
        let mut prev = MatrixState::<ROWS, COLS>::new();
        let mut raw = MatrixState::<ROWS, COLS>::new();
        raw.set(0, 0, true);

        process_scan(
            &raw,
            &mut prev,
            0,
            &mut PassthroughDebouncer,
            &mut proc,
            &mut Vec::new(),
        );

        assert_eq!(prev.get_pos(KeyPos::new(0, 0)), Some(true));
    }

    #[test]
    fn encoder_action_key_produces_keyboard_report() {
        let report = encoder_action_to_report::<()>(Action::Key(KeyCode::A));
        assert!(report.is_some());
        let codes = keycodes(&report.unwrap());
        assert_eq!(codes[0], KeyCode::A.usage_id());
    }

    #[test]
    fn encoder_action_key_release_clears_report() {
        let release = encoder_release_report::<()>(Action::Key(KeyCode::A));
        assert!(release.is_some());
        let codes = keycodes(&release.unwrap());
        assert_eq!(codes, [0; 6]);
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn encoder_action_consumer_produces_consumer_report() {
        use crate::action::ConsumerCode;
        let report = encoder_action_to_report::<()>(Action::Consumer(ConsumerCode::VolumeUp));
        assert!(report.is_some());
        match report.unwrap() {
            HidReport::Consumer(r) => assert_eq!(r.keycode, Some(ConsumerCode::VolumeUp)),
            _ => panic!("expected consumer report"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn encoder_action_consumer_release_clears() {
        use crate::action::ConsumerCode;
        let release = encoder_release_report::<()>(Action::Consumer(ConsumerCode::VolumeUp));
        assert!(release.is_some());
        match release.unwrap() {
            HidReport::Consumer(r) => assert_eq!(r.keycode, None),
            _ => panic!("expected consumer report"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn encoder_action_mouse_scroll_produces_mouse_report() {
        use crate::action::MouseAction;
        let report = encoder_action_to_report::<()>(Action::Mouse(MouseAction::Scroll(1)));
        assert!(report.is_some());
        match report.unwrap() {
            HidReport::Mouse(r) => {
                assert_eq!(r.scroll, 1);
                assert_eq!(r.dx, 0);
                assert_eq!(r.dy, 0);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    fn encoder_action_mouse_scroll_has_no_release() {
        use crate::action::MouseAction;
        let release = encoder_release_report::<()>(Action::Mouse(MouseAction::Scroll(1)));
        assert!(release.is_none());
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn encoder_action_mouse_move_produces_mouse_report() {
        use crate::action::MouseAction;
        let report =
            encoder_action_to_report::<()>(Action::Mouse(MouseAction::Move { dx: 5, dy: -3 }));
        match report.unwrap() {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, 5);
                assert_eq!(r.dy, -3);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    fn encoder_action_mouse_click_returns_none() {
        use crate::action::MouseAction;
        let report = encoder_action_to_report::<()>(Action::Mouse(MouseAction::Click(1)));
        assert!(report.is_none());
    }

    #[test]
    fn encoder_action_noop_returns_none() {
        let report = encoder_action_to_report::<()>(Action::NoOp);
        assert!(report.is_none());
    }

    #[test]
    fn encoder_release_noop_returns_none() {
        let release = encoder_release_report::<()>(Action::NoOp);
        assert!(release.is_none());
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn pointing_event_clamps_large_positive_dx() {
        let event = crate::pipeline::event::PointingEvent {
            dx: 200,
            dy: -200,
            buttons: 0x01,
            scroll: 2,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, 127);
                assert_eq!(r.dy, -128);
                assert_eq!(r.buttons, 0x01);
                assert_eq!(r.scroll, 2);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn pointing_event_preserves_small_values() {
        let event = crate::pipeline::event::PointingEvent {
            dx: 10,
            dy: -5,
            buttons: 0x03,
            scroll: -1,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, 10);
                assert_eq!(r.dy, -5);
                assert_eq!(r.buttons, 0x03);
                assert_eq!(r.scroll, -1);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn pointing_event_zero_movement() {
        let event = crate::pipeline::event::PointingEvent {
            dx: 0,
            dy: 0,
            buttons: 0,
            scroll: 0,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, 0);
                assert_eq!(r.dy, 0);
                assert_eq!(r.buttons, 0);
                assert_eq!(r.scroll, 0);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn pointing_event_boundary_values() {
        let event = crate::pipeline::event::PointingEvent {
            dx: -128,
            dy: 127,
            buttons: 0x07,
            scroll: 127,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, -128);
                assert_eq!(r.dy, 127);
                assert_eq!(r.buttons, 0x07);
                assert_eq!(r.scroll, 127);
            }
            _ => panic!("expected mouse report"),
        }
    }

    #[test]
    fn encoder_event_through_processor_produces_press_and_release() {
        use crate::devices::encoder::encoder_key_pos;
        use crate::pipeline::event::EncoderDirection;

        const ENC_ROWS: usize = 2;
        const ENC_COLS: usize = 3;
        const ENC_LAYERS: usize = 1;
        const ENC_KEYS: usize = 8; // 2*3 matrix + 1 encoder * 2

        let enc_cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS as u8);
        let enc_ccw_pos = encoder_key_pos(0, EncoderDirection::CounterClockwise, ENC_ROWS as u8);

        let mut layout = [Action::<()>::NoOp; ENC_KEYS];
        layout[enc_cw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::A);
        layout[enc_ccw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::B);

        let mut layers: heapless::Vec<crate::pipeline::keymap::Layer<()>, ENC_LAYERS> =
            heapless::Vec::new();
        let _ = layers.push(crate::pipeline::keymap::Layer::new(&layout));
        let mut proc: KeyboardProcessor<ENC_ROWS, ENC_COLS, ENC_LAYERS, ENC_KEYS> =
            KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default());

        let mut reports: heapless::Vec<HidReport, 16> = heapless::Vec::new();

        proc.process_event(KeyEvent::press(enc_cw_pos, 0), 0, &mut |r| {
            let _ = reports.push(r);
        });
        proc.process_event(KeyEvent::release(enc_cw_pos, 0), 0, &mut |r| {
            let _ = reports.push(r);
        });

        assert_eq!(reports.len(), 2);
        let press_codes = keycodes(&reports[0]);
        assert!(press_codes.contains(&KeyCode::A.usage_id()));
        let release_codes = keycodes(&reports[1]);
        assert!(!release_codes.contains(&KeyCode::A.usage_id()));
    }

    #[test]
    fn encoder_ccw_maps_to_different_action() {
        use crate::devices::encoder::encoder_key_pos;
        use crate::pipeline::event::EncoderDirection;

        const ENC_ROWS: usize = 2;
        const ENC_COLS: usize = 3;
        const ENC_LAYERS: usize = 1;
        const ENC_KEYS: usize = 8;

        let enc_cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS as u8);
        let enc_ccw_pos = encoder_key_pos(0, EncoderDirection::CounterClockwise, ENC_ROWS as u8);

        let mut layout = [Action::<()>::NoOp; ENC_KEYS];
        layout[enc_cw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::A);
        layout[enc_ccw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::B);

        let mut layers: heapless::Vec<crate::pipeline::keymap::Layer<()>, ENC_LAYERS> =
            heapless::Vec::new();
        let _ = layers.push(crate::pipeline::keymap::Layer::new(&layout));
        let mut proc: KeyboardProcessor<ENC_ROWS, ENC_COLS, ENC_LAYERS, ENC_KEYS> =
            KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default());

        let mut reports: heapless::Vec<HidReport, 16> = heapless::Vec::new();

        proc.process_event(KeyEvent::press(enc_ccw_pos, 0), 0, &mut |r| {
            let _ = reports.push(r);
        });

        assert_eq!(reports.len(), 1);
        let codes = keycodes(&reports[0]);
        assert!(codes.contains(&KeyCode::B.usage_id()));
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn pointing_event_buttons_only() {
        let event = crate::pipeline::event::PointingEvent {
            dx: 0,
            dy: 0,
            buttons: 0x05,
            scroll: 0,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.buttons, 0x05);
                assert_eq!(r.dx, 0);
                assert_eq!(r.dy, 0);
            }
            _ => panic!("expected mouse report"),
        }
    }
}
