//! Pointing device integration — trackball, trackpad, and analog stick input.
//!
//! On Embassy targets, implement `AsyncPointingScanner` to poll a pointing
//! device and produce [`PointingEvent`](crate::pipeline::event::PointingEvent)s.
//! The async `pointing_task` converts these into
//! [`HidReport::Mouse`](crate::pipeline::report::HidReport::Mouse) reports.
//!
//! See also: [`action::MouseAction`](crate::action::MouseAction) for
//! key-triggered mouse actions, [`encoder`](crate::devices::encoder) for rotary input.

/// Async scanner trait for pointing devices.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncPointingScanner {
    /// Waits for and returns the next pointing event.
    async fn scan(&mut self) -> crate::pipeline::event::PointingEvent;
}

#[cfg(test)]
mod tests {
    use crate::pipeline::event::PointingEvent;

    #[test]
    fn pointing_event_default_values() {
        let e = PointingEvent {
            dx: 0,
            dy: 0,
            buttons: 0,
            scroll: 0,
        };
        assert_eq!(e.dx, 0);
        assert_eq!(e.dy, 0);
    }

    #[test]
    fn pointing_event_negative_movement() {
        let e = PointingEvent {
            dx: -100,
            dy: -200,
            buttons: 0,
            scroll: -1,
        };
        assert_eq!(e.dx, -100);
        assert_eq!(e.dy, -200);
        assert_eq!(e.scroll, -1);
    }
}
