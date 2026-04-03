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

use crate::debounce::Debouncer;
use crate::event::KeyEvent;
use crate::hid::AsyncReporter;
use crate::keyboard::KeyboardProcessor;
use crate::matrix::{AsyncMatrixScanner, MatrixState};
use crate::report::HidReport;

/// A channel for routing [`KeyEvent`]s from the matrix task to the keyboard task.
///
/// `M` is the raw mutex (use `NoopRawMutex` for single-core Embassy).
/// `N` is the channel capacity; 16 is a reasonable default.
pub type EventChannel<M, const N: usize> = Channel<M, KeyEvent, N>;

/// A channel for routing [`HidReport`]s from the keyboard task to the USB task.
///
/// `M` is the raw mutex. `N` is the channel capacity; 8 is a reasonable default.
pub type ReportChannel<M, const N: usize> = Channel<M, HidReport, N>;

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
        let event_opt = if let Some(deadline_ms) = processor.next_deadline_ms() {
            let now_ms = Instant::now().as_millis();
            let remaining = Duration::from_millis(deadline_ms.saturating_sub(now_ms));
            match with_timeout(remaining, event_rx.receive()).await {
                Err(_timeout) => {
                    let tick_ms = Instant::now().as_millis();
                    processor.tick(tick_ms, &mut |r| {
                        report_tx.try_send(r).ok();
                    });
                    None
                }
                Ok(ev) => Some(ev),
            }
        } else {
            Some(event_rx.receive().await)
        };

        if let Some(event) = event_opt {
            let now_ms = Instant::now().as_millis();
            processor.process_event(event, now_ms, &mut |r| {
                report_tx.try_send(r).ok();
            });
        }
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
            reporter.send(&report).await.ok();
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
                        reports.push(r).ok();
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
            reporter.send(report).await.ok();
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
                    reports.push(r).ok();
                });
            }
        }
        *prev = stable;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, KeyCode};
    use crate::debounce::Debouncer;
    use crate::event::KeyPos;
    use crate::keymap::{Layer, LayerStack};
    use crate::matrix::MatrixState;
    use crate::report::HidReport;
    use crate::tapping::TapHoldConfig;

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
        layers.push(Layer::new(base)).ok();
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
            HidReport::Consumer(_) | HidReport::NkroKeyboard(_) => [0; 6],
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
}
