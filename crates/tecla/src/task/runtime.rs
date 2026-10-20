//! Async Embassy entry point.
//!
//! [`run()`] wires the full keyboard pipeline: matrix scan → debounce →
//! [`KeyboardProcessor`] → [`AsyncReporter`]. Call it from your board crate's
//! Embassy entry point and never return.
//!
//! # Example
//!
//! Sketch — the scanner and reporter are board-specific, so this is not a
//! compilable example. See `examples/` in any `tecla-*` board crate.
//!
//! ```text
//! #[embassy_executor::main]
//! async fn main(_spawner: Spawner) {
//!     let scanner  = MyMatrixScanner::new(/* GPIO pins */);
//!     let debouncer = AsymEagerDefer::new(5);
//!     let processor = KeyboardProcessor::new(MY_LAYERS, TapHoldConfig::default());
//!     let reporter  = MyUsbReporter::new(/* USB driver */);
//!     tecla::task::run(scanner, debouncer, &mut processor, reporter).await;
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
use embassy_time::{with_timeout, Duration, Instant, Ticker};
use heapless::Vec;

use crate::action::Action;
#[cfg(feature = "autocorrect")]
use crate::action::KeyCode;
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
/// `N` is the channel capacity. 8 is a reasonable default for keyboard state;
/// MIDI events and relative mouse movement must all be delivered. Increase it
/// when bursts can outpace the transport.
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
    let mut ticker = Ticker::every(Duration::from_millis(MATRIX_SCAN_MS));
    loop {
        ticker.next().await;
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
) -> Option<KeyEvent> {
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
                    let mut reports: Vec<HidReport, 64> = Vec::new();
                    processor.tick(tick_ms, &mut |r| {
                        let _ = reports.push(r);
                    });
                    send_reports(report_tx, reports).await;
                }
                None
            }
            Ok(ev) => Some(ev),
        },
        None => Some(event_rx.receive().await),
    };

    if let Some(event) = event_opt {
        let ev_ms = Instant::now().as_millis();
        let mut reports: Vec<HidReport, 64> = Vec::new();
        processor.process_event(event, ev_ms, &mut |r| {
            let _ = reports.push(r);
        });
        send_reports(report_tx, reports).await;
        return Some(event);
    }
    None
}

async fn send_reports<M: RawMutex, const N: usize>(
    report_tx: &Sender<'_, M, HidReport, N>,
    reports: Vec<HidReport, 64>,
) {
    for report in reports {
        report_tx.send(report).await;
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
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, processor, &report_tx, None).await;
    }
}

/// Like [`keyboard_task`] but also delivers audio and haptic feedback.
///
/// Pair with [`audio_task`] and [`haptic_task`] on the other end of the
/// channels. Without this variant, `Action::Audio` and `Action::Haptic` park a
/// command on the processor that nothing ever collects.
///
/// Pass `None` for a channel the board does not have; the keycode then logs a
/// warning instead of silently doing nothing.
pub async fn keyboard_task_feedback<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const AN: usize,
    const HN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    audio_tx: Option<Sender<'_, M, crate::audio::AudioCommand, AN>>,
    haptic_tx: Option<Sender<'_, M, crate::haptic::HapticEffect, HN>>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, processor, &report_tx, None).await;
        let _ = drain_feedback(processor, audio_tx.as_ref(), haptic_tx.as_ref());
    }
}

/// Applies one pending Via command, if the host sent one.
///
/// The USB half runs in `transport::usb::via_usb_task` (feature `usb`);
/// this is the half that needs `&mut KeyboardProcessor`, so it runs inside the
/// keyboard task where the processor already lives. Non-blocking: returns
/// immediately when no command is waiting, so it is safe to call every loop.
///
/// Returns `true` if a command was handled.
#[cfg(feature = "via")]
pub fn service_via<
    M: RawMutex,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext: Copy + PartialEq + 'static,
    const N: usize,
>(
    via: &mut crate::storage::via::ViaProtocol<COLS>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    req_rx: &Receiver<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], N>,
    resp_tx: &Sender<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], N>,
) -> bool {
    let Ok(request) = req_rx.try_receive() else {
        return false;
    };
    via.set_uptime(Instant::now().as_millis());
    let mut response = [0u8; crate::pipeline::report::VIA_REPORT_SIZE];
    via.process_command(&request, &mut response, processor);
    if resp_tx.try_send(response).is_err() {
        // The USB half is always waiting on this channel when it has a request
        // outstanding, so a full channel means the host went away mid-exchange.
        defmt::warn!("via: response dropped, host not reading");
    }
    true
}

/// Like [`keyboard_task`] but also services Via keymap-editing commands.
///
/// Pair with `transport::usb::via_usb_task` (feature `usb`).
#[cfg(feature = "via")]
pub async fn keyboard_task_via<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const VN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    via: &mut crate::storage::via::ViaProtocol<COLS>,
    via_req_rx: Receiver<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], VN>,
    via_resp_tx: Sender<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], VN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq + 'static,
{
    loop {
        // Bound the event wait so a Via command sent while the keyboard is
        // idle is still answered promptly rather than waiting for a keypress.
        receive_and_process(&event_rx, processor, &report_tx, Some(VIA_POLL_MS)).await;
        service_via(via, processor, &via_req_rx, &via_resp_tx);
    }
}

/// How long the Via-enabled keyboard task will block on key events before
/// checking for a pending host command.
#[cfg(feature = "via")]
const VIA_POLL_MS: u64 = 10;

/// Matrix scan interval.
///
/// Every scan loop is paced by this. Left free-running they scan as fast as the
/// hardware allows — tens of thousands of times a second on a GPIO matrix, with
/// the MCU never idling, and on a board whose other half is an I2C expander the
/// scan consumes every byte of bus time it can get and starves anything sharing
/// that bus. Debounce windows are 5-12ms and the debouncer quantises to
/// milliseconds, so the discarded scans bought nothing: pacing costs no latency.
const MATRIX_SCAN_MS: u64 = 1;

/// Drains the feedback commands a keymap produced into their hardware tasks.
///
/// [`Action::Audio`](crate::action::Action::Audio),
/// [`Action::Haptic`](crate::action::Action::Haptic) and
/// [`Action::OutputSelect`](crate::action::Action::OutputSelect) park a pending
/// command on the processor rather than acting directly, because the hardware
/// they drive lives in other tasks. Something has to move them across, or the
/// keycodes do nothing at all — call this every loop iteration.
///
/// Uses `try_send`, so a full channel drops the command rather than stalling
/// the real-time key path. A dropped beep is better than a dropped keystroke.
/// Returns any pending [`OutputMode`](crate::pipeline::hid::OutputMode) switch
/// for the caller to apply to its
/// [`DualReporter`](crate::pipeline::hid::DualReporter).
pub fn drain_feedback<
    M: RawMutex,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext: Copy + PartialEq,
    const AN: usize,
    const HN: usize,
>(
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    audio_tx: Option<&Sender<'_, M, crate::audio::AudioCommand, AN>>,
    haptic_tx: Option<&Sender<'_, M, crate::haptic::HapticEffect, HN>>,
) -> Option<crate::pipeline::hid::OutputMode> {
    if let Some(cmd) = processor.pending_audio_command() {
        match audio_tx {
            Some(tx) => {
                if tx.try_send(cmd).is_err() {
                    defmt::warn!("audio command dropped: channel full");
                }
            }
            None => defmt::warn!("Action::Audio used but no audio task is wired up"),
        }
    }
    // Toggle is controller state, not an effect to play; only Play carries
    // something the driver can act on.
    if let Some(crate::haptic::HapticCommand::Play(effect)) = processor.pending_haptic_command() {
        {
            match haptic_tx {
                Some(tx) => {
                    if tx.try_send(effect).is_err() {
                        defmt::warn!("haptic effect dropped: channel full");
                    }
                }
                None => defmt::warn!("Action::Haptic used but no haptic task is wired up"),
            }
        }
    }
    processor.pending_output_mode()
}

/// Runs an autocorrect engine off the keyboard's real-time path.
///
/// Receives typed [`KeyCode`]s on `kc_rx`, feeds them to `engine`, and sends any
/// [`Correction`](crate::autocorrect::Correction) back on `corr_tx`. Pair with
/// [`keyboard_task_autocorrect`] and
/// [`KeyboardProcessor::with_autocorrect_offload`].
///
/// Spawn this on whichever executor you like: on a single-core target run it on
/// the same executor as [`keyboard_task_autocorrect`] (use `NoopRawMutex`); on
/// RP2040 spawn it on core1 via `embassy_rp::multicore::spawn_core1` (use
/// `CriticalSectionRawMutex` for the channels). The task body is identical
/// either way — the "core1 split" is purely a spawn-site choice.
///
/// This function never returns.
#[cfg(feature = "autocorrect")]
pub async fn autocorrect_task<M, const KN: usize, const CN: usize>(
    kc_rx: Receiver<'_, M, KeyCode, KN>,
    corr_tx: Sender<'_, M, crate::autocorrect::Correction, CN>,
    mut engine: crate::autocorrect::AutocorrectEngine,
) -> !
where
    M: RawMutex,
{
    loop {
        let kc = kc_rx.receive().await;
        if let Some(correction) = engine.process_keycode(kc, Instant::now().as_millis()) {
            corr_tx.send(correction).await;
        }
    }
}

/// Like [`keyboard_task`] but offloads autocorrect to [`autocorrect_task`].
///
/// After processing each event it forwards any typed keycodes to `kc_tx` (the
/// engine's input) and applies any pending corrections from `corr_rx`. The
/// processor must be built with
/// [`with_autocorrect_offload`](KeyboardProcessor::with_autocorrect_offload).
///
/// This function never returns.
#[cfg(feature = "autocorrect")]
pub async fn keyboard_task_autocorrect<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const KN: usize,
    const CN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    kc_tx: Sender<'_, M, KeyCode, KN>,
    corr_rx: Receiver<'_, M, crate::autocorrect::Correction, CN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, processor, &report_tx, None).await;

        // Forward typed keycodes to the (possibly cross-core) engine.
        // ponytail: try_send on a bounded channel — overflow drops a feed char
        // (a missed/false correction that self-heals on the next word boundary),
        // but the real-time core0 path is never blocked. Widen KN if it matters.
        while let Some(kc) = processor.pop_typed_keycode() {
            let _ = kc_tx.try_send(kc);
        }

        // Apply any corrections the engine produced since the last event.
        let mut reports: Vec<HidReport, 64> = Vec::new();
        while let Ok(correction) = corr_rx.try_receive() {
            let mut replacement: Vec<KeyCode, 32> = Vec::new();
            for &uid in correction.replacement_slice() {
                if let Some(kc) = KeyCode::from_usage_id(uid) {
                    let _ = replacement.push(kc);
                }
            }
            processor.inject_correction(
                usize::from(correction.backspaces),
                &replacement,
                &mut |r| {
                    let _ = reports.push(r);
                },
            );
        }
        send_reports(&report_tx, reports).await;
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
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    ble_tx: Sender<'_, M, crate::transport::ble::BleCommand, BN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        receive_and_process(&event_rx, processor, &report_tx, None).await;
        if let Some(cmd) = processor.pending_ble_command() {
            let _ = ble_tx.try_send(cmd);
        }
    }
}

/// Like [`keyboard_task`] but also forwards raw key presses and pending RGB
/// commands to a board-owned RGB task.
///
/// Every press is forwarded to `rgb_event_tx` (releases are not, since the
/// built-in [`Animation`](crate::rgb::Animation)s only react to presses), and
/// any [`RgbCommand`](crate::rgb::RgbCommand) yielded by an [`Action::Rgb`]
/// key is forwarded to `rgb_cmd_tx`. Pair with a board task that owns the
/// [`RgbController`](crate::rgb::RgbController), drains both channels, and
/// ticks the animation on a timer.
///
/// This function never returns.
pub async fn keyboard_task_rgb<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const EN2: usize,
    const CN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    rgb_event_tx: Sender<'_, M, KeyEvent, EN2>,
    rgb_cmd_tx: Sender<'_, M, crate::rgb::RgbCommand, CN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq,
{
    loop {
        if let Some(event) = receive_and_process(&event_rx, processor, &report_tx, None).await {
            if event.pressed {
                let _ = rgb_event_tx.try_send(event);
            }
        }
        if let Some(cmd) = processor.pending_rgb_command() {
            let _ = rgb_cmd_tx.try_send(cmd);
        }
    }
}

/// Like [`keyboard_task_rgb`], but also answers Via host commands.
///
/// Both behaviours have to live in one task because both need `&mut` on the
/// processor: RGB reads the pending command it produced, Via reads and rewrites
/// its keymap. Pair with `transport::usb::via_usb_task` (feature `usb`).
#[cfg(feature = "via")]
#[expect(clippy::too_many_arguments, reason = "one task, two host-facing roles")]
pub async fn keyboard_task_rgb_via<
    M,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext,
    const EN: usize,
    const RN: usize,
    const EN2: usize,
    const CN: usize,
    const VN: usize,
>(
    event_rx: Receiver<'_, M, KeyEvent, EN>,
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    report_tx: Sender<'_, M, HidReport, RN>,
    rgb_event_tx: Sender<'_, M, KeyEvent, EN2>,
    rgb_cmd_tx: Sender<'_, M, crate::rgb::RgbCommand, CN>,
    via: &mut crate::storage::via::ViaProtocol<COLS>,
    via_req_rx: Receiver<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], VN>,
    via_resp_tx: Sender<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], VN>,
) -> !
where
    M: RawMutex,
    Ext: Copy + PartialEq + 'static,
{
    loop {
        // Bounded wait, as in `keyboard_task_via`: a host command sent while
        // the keyboard is idle must not wait for the next keypress.
        if let Some(event) =
            receive_and_process(&event_rx, processor, &report_tx, Some(VIA_POLL_MS)).await
        {
            if event.pressed {
                let _ = rgb_event_tx.try_send(event);
            }
        }
        if let Some(cmd) = processor.pending_rgb_command() {
            let _ = rgb_cmd_tx.try_send(cmd);
        }
        service_via(via, processor, &via_req_rx, &via_resp_tx);
    }
}

/// Like [`keyboard_task`] but periodically collects telemetry snapshots and
/// sends them to `telem_tx`.
///
/// The telemetry interval is 250ms. If the processor has telemetry enabled
/// (via [`KeyboardProcessor::with_telemetry`]), a
/// [`TelemetryFrame`](crate::telemetry::transport::TelemetryFrame) is
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
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
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
        receive_and_process(&event_rx, processor, &report_tx, Some(telem_remaining)).await;

        let now = Instant::now().as_millis();
        if now.saturating_sub(last_telem_ms) >= TELEM_INTERVAL_MS {
            if let Some(t) = processor.telemetry_mut() {
                // ponytail: encoders are scanned in their own task with their
                // own collector; boards that want encoder metrics in the frame
                // call collect_frame directly with that snapshot.
                let frame = crate::telemetry::transport::collect_frame(t, None);
                let _ = telem_tx.try_send(frame);
            }
            last_telem_ms = now;
        }
    }
}

/// Receives [`TelemetryFrame`](crate::telemetry::transport::TelemetryFrame)s
/// and writes serialized bytes to an
/// [`AsyncTelemetryWriter`](crate::telemetry::transport::AsyncTelemetryWriter).
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
/// Identical consecutive state reports are coalesced after successful delivery.
/// MIDI events and relative mouse movement are always sent, including repeats.
/// A failed send invalidates the cache so a subsequent matching report is retried.
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
        let state_report = matches!(
            report,
            HidReport::Keyboard(_)
                | HidReport::NkroKeyboard(_)
                | HidReport::Consumer(_)
                | HidReport::Gamepad(_)
        );
        if !state_report || Some(report) != last {
            match reporter.send(&report).await {
                Ok(()) => last = Some(report),
                Err(e) => {
                    last = None;
                    defmt::warn!("usb_task: report dropped: {}", defmt::Debug2Format(&e));
                }
            }
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
/// This is the single-task path: there are no channels, so audio and haptic
/// feedback have nowhere to go. `Action::Audio` / `Action::Haptic` in a keymap
/// run on this path log a warning rather than doing nothing quietly — use the
/// task topology with [`keyboard_task_feedback`] if the board has a buzzer or
/// a haptic driver.
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
    processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    mut reporter: R,
) -> !
where
    S: AsyncMatrixScanner<ROWS, COLS>,
    Db: Debouncer<ROWS, COLS>,
    R: AsyncReporter,
    Ext: Copy + PartialEq,
{
    let mut prev: MatrixState<ROWS, COLS> = MatrixState::new();
    let mut ticker = Ticker::every(Duration::from_millis(MATRIX_SCAN_MS));
    loop {
        ticker.next().await;
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
                processor,
                &mut reports,
            );
        }

        for report in &reports {
            if let Err(e) = reporter.send(report).await {
                defmt::warn!("run: report dropped: {}", defmt::Debug2Format(&e));
            }
        }

        // No channels on this path — this warns if a keymap uses a feedback
        // action the single-task topology cannot deliver.
        let _ =
            drain_feedback::<embassy_sync::blocking_mutex::raw::NoopRawMutex, _, _, _, _, _, 1, 1>(
                processor, None, None,
            );
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
/// event at a virtual [`KeyPos`](crate::pipeline::event::KeyPos) computed by
/// [`encoder_key_pos`]. The user's
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
                    dx: dx.into(),
                    dy: dy.into(),
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

/// Converts a [`PointingEvent`](crate::pipeline::event::PointingEvent) into a
/// [`HidReport::Mouse`](crate::pipeline::report::HidReport::Mouse).
///
/// Movement passes through at full 16-bit width — the report carries it.
#[must_use]
pub fn pointing_event_to_report(event: &crate::pipeline::event::PointingEvent) -> HidReport {
    HidReport::Mouse(MouseReport {
        buttons: event.buttons,
        dx: event.dx,
        dy: event.dy,
        scroll: event.scroll,
    })
}

/// Scans a pointing device and emits [`HidReport::Mouse`] reports.
///
/// Movement passes through at full 16-bit width.
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
    let mut ticker = Ticker::every(Duration::from_millis(MATRIX_SCAN_MS));
    loop {
        ticker.next().await;
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

    fn deliver_usb_reports(reports: &[HidReport], fail_first: bool) -> std::vec::Vec<HidReport> {
        use core::cell::RefCell;
        use core::future::Future;
        use core::task::{Context, Waker};
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;

        struct RecordingReporter<'a> {
            attempts: &'a RefCell<std::vec::Vec<HidReport>>,
            fail_next: bool,
        }

        impl AsyncReporter for RecordingReporter<'_> {
            async fn send(
                &mut self,
                report: &HidReport,
            ) -> Result<(), crate::pipeline::hid::ReportError> {
                self.attempts.borrow_mut().push(*report);
                if core::mem::take(&mut self.fail_next) {
                    Err(crate::pipeline::hid::ReportError::NotConnected)
                } else {
                    Ok(())
                }
            }
        }

        let channel = Channel::<NoopRawMutex, HidReport, 8>::new();
        for report in reports {
            channel.try_send(*report).unwrap();
        }
        let attempts = RefCell::new(std::vec::Vec::new());
        {
            let reporter = RecordingReporter {
                attempts: &attempts,
                fail_next: fail_first,
            };
            let mut task = core::pin::pin!(usb_task(channel.receiver(), reporter));
            let mut context = Context::from_waker(Waker::noop());
            assert!(task.as_mut().poll(&mut context).is_pending());
        }
        attempts.into_inner()
    }

    #[test]
    fn usb_delivery_preserves_repeated_midi_events() {
        use crate::pipeline::report::MidiEvent;

        let on = HidReport::Midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        let off = HidReport::Midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
        });
        let reports = [on, on, off, off];
        assert_eq!(deliver_usb_reports(&reports, false), reports);
    }

    #[test]
    fn usb_delivery_preserves_repeated_relative_movement() {
        let movement = HidReport::Mouse(MouseReport {
            dx: 4,
            ..MouseReport::default()
        });
        let reports = [movement, movement];
        assert_eq!(deliver_usb_reports(&reports, false), reports);
    }

    #[test]
    fn usb_delivery_coalesces_successful_state_reports() {
        use crate::pipeline::report::{GamepadReport, NkroKeyboardReport};

        let states = [
            HidReport::Keyboard(KeyboardReport::new()),
            HidReport::NkroKeyboard(NkroKeyboardReport::new()),
            HidReport::Consumer(ConsumerReport::new()),
            HidReport::Gamepad(GamepadReport::default()),
        ];
        let mut reports = std::vec::Vec::new();
        for state in states {
            reports.extend([state, state]);
        }
        assert_eq!(deliver_usb_reports(&reports, false), states);
    }

    #[test]
    fn usb_delivery_does_not_cache_failed_sends() {
        let report = HidReport::Keyboard(KeyboardReport::new());
        assert_eq!(
            deliver_usb_reports(&[report, report, report], true),
            [report, report]
        );
    }

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
        let codes = keycodes(&report.unwrap());
        assert_eq!(codes[0], KeyCode::A.usage_id());
    }

    #[test]
    fn encoder_action_key_release_clears_report() {
        let release = encoder_release_report::<()>(Action::Key(KeyCode::A));
        let codes = keycodes(&release.unwrap());
        assert_eq!(codes, [0; 6]);
    }

    #[test]
    #[expect(clippy::panic, reason = "test code")]
    fn encoder_action_consumer_produces_consumer_report() {
        use crate::action::ConsumerCode;
        let report = encoder_action_to_report::<()>(Action::Consumer(ConsumerCode::VolumeUp));
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
    fn pointing_event_passes_large_movement_through() {
        let event = crate::pipeline::event::PointingEvent {
            dx: 200,
            dy: -200,
            buttons: 0x01,
            scroll: 2,
        };
        let report = pointing_event_to_report(&event);
        match report {
            HidReport::Mouse(r) => {
                assert_eq!(r.dx, 200);
                assert_eq!(r.dy, -200);
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
