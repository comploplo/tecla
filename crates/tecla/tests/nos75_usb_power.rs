#![allow(
    missing_docs,
    clippy::tests_outside_test_module,
    reason = "board power-state integration tests"
)]
#[path = "../../../keyboards/tecla/nos75/usb_power.rs"]
mod usb_power;
use usb_power::UsbPower;

#[test]
fn configuration_sleep_resume_and_disconnect_control_lighting() {
    let mut power = UsbPower::default();
    assert!(!power.lighting_allowed());
    power.enabled(true);
    assert!(!power.lighting_allowed());
    power.configured(true);
    assert!(power.lighting_allowed());
    power.enabled(true);
    assert!(power.lighting_allowed());
    power.suspended(true);
    assert!(!power.lighting_allowed());
    power.suspended(false);
    assert!(power.lighting_allowed());
    power.configured(false);
    power.suspended(false);
    assert!(!power.lighting_allowed());
    power.configured(true);
    power.enabled(false);
    assert!(!power.lighting_allowed());
    power.enabled(true);
    assert!(!power.lighting_allowed());
}

#[test]
fn bus_reset_clears_previous_configuration_and_suspend() {
    let mut power = UsbPower::default();
    power.configured(true);
    power.suspended(true);
    power.reset();
    assert!(!power.lighting_allowed());
    power.configured(true);
    assert!(power.lighting_allowed());
}

#[test]
fn disconnect_while_suspended_does_not_leave_the_next_connection_asleep() {
    let mut power = UsbPower::default();
    power.configured(true);
    power.suspended(true);
    power.enabled(false);
    power.enabled(true);
    assert!(!power.lighting_allowed());
    power.configured(true);
    assert!(power.lighting_allowed());
}
