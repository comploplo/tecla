//! Battery voltage monitoring via the nRF52840 SAADC.
//!
//! Reads VDD through the internal voltage divider and converts to a
//! [`BatteryLevel`] for the BLE Battery Service characteristic.

use embassy_nrf::saadc::{self, ChannelConfig, Config, Saadc, VddInput};
use embassy_nrf::{bind_interrupts, peripherals, Peri};
use embassy_time::Timer;
use tecla::pipeline::power::BatteryLevel;

use crate::ble_reporter::HidGattServer;

bind_interrupts!(struct SaadcIrqs {
    SAADC => saadc::InterruptHandler;
});

/// ADC full-scale with 1/6 gain and 0.6V reference is 3600mV.
/// 100% means "at or above 3.6V" — likely fully charged for a LiPo cell.
const VBATT_FULL_MV: u16 = 3600;
const VBATT_EMPTY_MV: u16 = 3000;

/// Reads battery voltage from the nRF52840 internal VDD channel.
pub struct BatteryMonitor<'d> {
    saadc: Saadc<'d, 1>,
}

impl<'d> BatteryMonitor<'d> {
    /// Creates a new monitor using the SAADC peripheral and internal VDD input.
    pub fn new(saadc_peripheral: Peri<'d, peripherals::SAADC>, vdd: VddInput) -> Self {
        let config = Config::default();
        let channel = ChannelConfig::single_ended(vdd);
        let saadc = Saadc::new(saadc_peripheral, SaadcIrqs, config, [channel]);
        Self { saadc }
    }

    /// Samples VDD and returns the current battery level.
    #[expect(
        clippy::as_conversions,
        reason = "ADC raw value to millivolt conversion"
    )]
    pub async fn read(&mut self) -> BatteryLevel {
        let mut buf = [0i16; 1];
        self.saadc.sample(&mut buf).await;
        let raw = buf[0].max(0) as u32;
        let millivolts = ((raw * 3600) / 4096) as u16;
        let percent = millivolts_to_percent(millivolts);
        BatteryLevel::new(percent, millivolts)
    }
}

#[expect(
    clippy::as_conversions,
    reason = "millivolt range math widened to u32 then narrowed to u8 percent"
)]
fn millivolts_to_percent(mv: u16) -> u8 {
    if mv >= VBATT_FULL_MV {
        return 100;
    }
    if mv <= VBATT_EMPTY_MV {
        return 0;
    }
    let range = (VBATT_FULL_MV - VBATT_EMPTY_MV) as u32;
    let above_empty = (mv - VBATT_EMPTY_MV) as u32;
    ((above_empty * 100) / range) as u8
}

/// Embassy task that reads battery level every 60 seconds and updates the
/// BLE Battery Service characteristic when the percentage changes.
#[embassy_executor::task]
pub async fn battery_task(
    mut monitor: BatteryMonitor<'static>,
    server: &'static HidGattServer<'static>,
) -> ! {
    let mut last_percent: u8 = 100;
    loop {
        Timer::after_secs(60).await;
        let level = monitor.read().await;
        if level.percent != last_percent {
            last_percent = level.percent;
            server.set_battery_level(level.percent);
            defmt::info!("[battery] {}% ({}mV)", level.percent, level.millivolts);
        }
    }
}
