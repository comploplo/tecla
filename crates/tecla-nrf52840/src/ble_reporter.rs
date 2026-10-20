//! BLE HID reporter for nRF52840 using the `trouble-host` BLE stack.
//!
//! [`BleHidReporter`] implements both [`AsyncReporter`] and [`BleReporter`]
//! by wrapping a trouble-host GATT server with the HID-over-GATT profile.
//!
//! # GATT services
//!
//! - **HID Service** (0x1812) — keyboard input report, report map, HID info,
//!   protocol mode
//! - **Battery Service** (0x180F) — battery level characteristic
//! - **Device Information Service** (0x180A) — manufacturer, model, firmware rev
//!
//! # Build requirements
//!
//! The `nrf-sdc` build runs bindgen against CMSIS C headers. Set
//! `LIBCLANG_PATH=/usr/lib/llvm-18/lib` (or your system's libclang path) so
//! that clang can locate `stdint.h` for the `thumbv7em-none-eabihf` target.

use trouble_host::prelude::*;

use tecla::pipeline::hid::{AsyncReporter, ReportError};
use tecla::pipeline::report::HidReport;
use tecla::transport::ble::BleError;

const HID_REPORT_MAP: &[u8] = crate::KEYBOARD_REPORT_DESCRIPTOR;

#[gatt_service(uuid = "1812")]
struct HidService {
    #[characteristic(uuid = "2a4d", read, notify, value = [0u8; 9])]
    #[descriptor(uuid = "2908", read, value = [0x01u8, 0x01])]
    input_report: [u8; 9],
    #[characteristic(uuid = "2a4d", read, notify, value = [0u8; 3])]
    #[descriptor(uuid = "2908", read, value = [0x02u8, 0x01])]
    consumer_report: [u8; 3],
    #[characteristic(uuid = "2a4b", read, value = HID_REPORT_MAP)]
    report_map: &'static [u8],
    #[characteristic(uuid = "2a4a", read, value = [0x01u8, 0x01, 0x00, 0x02])]
    hid_information: [u8; 4],
    #[characteristic(uuid = "2a4e", read, write, value = 1u8)]
    protocol_mode: u8,
}

#[gatt_service(uuid = "180f")]
struct BatteryService {
    #[characteristic(uuid = "2a19", read, notify, value = 100u8)]
    level: u8,
}

#[gatt_service(uuid = "180a")]
struct DeviceInfoService {
    #[characteristic(uuid = "2a29", read, value = "Tecla")]
    manufacturer: &'static str,
    #[characteristic(uuid = "2a24", read, value = "Keyboard")]
    model: &'static str,
    #[characteristic(uuid = "2a26", read, value = "0.1.0")]
    firmware_rev: &'static str,
}

#[gatt_server]
pub(crate) struct HidGattServer {
    hid: HidService,
    battery: BatteryService,
    device_info: DeviceInfoService,
}

impl<'a> HidGattServer<'a> {
    pub(crate) fn set_battery_level(&self, percent: u8) {
        let _ = self.set(&self.battery.level, &percent);
    }
}

/// Wraps a trouble-host GATT connection to send HID keyboard reports over BLE.
pub(crate) struct BleHidReporter<'a, 'server, P: PacketPool> {
    server: &'server HidGattServer<'a>,
    conn: Option<GattConnection<'a, 'server, P>>,
}

impl<'a, 'server, P: PacketPool> BleHidReporter<'a, 'server, P> {
    pub(crate) fn new(server: &'server HidGattServer<'a>) -> Self {
        Self { server, conn: None }
    }

    pub(crate) fn set_connection(&mut self, conn: GattConnection<'a, 'server, P>) {
        self.conn = Some(conn);
    }

    pub(crate) fn clear_connection(&mut self) {
        self.conn = None;
    }

    fn serialize_keyboard_report(kr: &tecla::pipeline::report::KeyboardReport) -> [u8; 9] {
        [
            0x01,
            kr.modifiers,
            kr.reserved,
            kr.keycodes[0],
            kr.keycodes[1],
            kr.keycodes[2],
            kr.keycodes[3],
            kr.keycodes[4],
            kr.keycodes[5],
        ]
    }

    #[expect(
        clippy::as_conversions,
        reason = "consumer keycode enum to u16 wire bytes"
    )]
    fn serialize_consumer_report(cr: &tecla::pipeline::report::ConsumerReport) -> [u8; 3] {
        let usage = cr.keycode.map_or(0u16, |k| k as u16);
        [0x02, usage as u8, (usage >> 8) as u8]
    }
}

impl<'a, 'server, P: PacketPool> AsyncReporter for BleHidReporter<'a, 'server, P> {
    async fn send(&mut self, report: &HidReport) -> Result<(), ReportError> {
        let conn = self.conn.as_ref().ok_or(ReportError::NotConnected)?;
        match report {
            HidReport::Keyboard(kr) => {
                let bytes = Self::serialize_keyboard_report(kr);
                self.server
                    .hid
                    .input_report
                    .notify(conn, &bytes, false)
                    .await
                    .map_err(|_| ReportError::NotConnected)
            }
            HidReport::Consumer(cr) => {
                let bytes = Self::serialize_consumer_report(cr);
                self.server
                    .hid
                    .consumer_report
                    .notify(conn, &bytes, false)
                    .await
                    .map_err(|_| ReportError::NotConnected)
            }
            _ => Ok(()),
        }
    }
}

impl<'a, 'server, P: PacketPool> tecla::transport::ble::BleReporter
    for BleHidReporter<'a, 'server, P>
{
    /// No-op: connection acceptance is driven by `run_ble_peripheral`'s
    /// advertise-accept loop, not by the trait caller.
    async fn connect(&mut self) -> Result<(), BleError> {
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), BleError> {
        self.conn = None;
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// No-op: advertising is managed by `run_ble_peripheral`'s main loop,
    /// which handles both directed (bonded) and undirected advertising.
    async fn advertise(&mut self) -> Result<(), BleError> {
        Ok(())
    }
}

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;

/// Run the BLE peripheral event loop: advertise, accept connections, and serve
/// the HID GATT server until the connection drops, then re-advertise.
#[expect(
    clippy::unwrap_used,
    reason = "GATT server creation and active connection are infallible at this point"
)]
pub async fn run_ble_peripheral<C: Controller>(
    controller: C,
    name: &str,
    active_bond: Option<&tecla::transport::ble::BondInfo>,
    ble_address: [u8; 6],
) -> ! {
    let address = Address::random(ble_address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    // No RNG parameter: trouble-host seeds its security-manager RNG itself
    // from the controller's LeRand HCI command, which is a better entropy
    // source than a caller-supplied peripheral anyway.
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .build();
    stack.set_io_capabilities(IoCapabilities::NoInputNoOutput);
    let mut peripheral = stack.peripheral();
    let runner = stack.runner();

    let server = HidGattServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name,
        appearance: &appearance::human_interface_device::KEYBOARD,
    }))
    .unwrap();

    let mut reporter = BleHidReporter::<DefaultPacketPool>::new(&server);

    let _ = embassy_futures::join::join(ble_runner_task(runner), async {
        loop {
            let conn_result = if let Some(bond) = active_bond {
                let peer = Address::random(bond.address);
                match advertise_directed(peer, &mut peripheral, &server).await {
                    ok @ Ok(_) => ok,
                    Err(_) => advertise_undirected(name, &mut peripheral, &server).await,
                }
            } else {
                advertise_undirected(name, &mut peripheral, &server).await
            };

            match conn_result {
                Ok(conn) => {
                    reporter.set_connection(conn);
                    let events = gatt_event_loop(&server, reporter.conn.as_ref().unwrap());
                    events.await;
                    reporter.clear_connection();
                }
                Err(_) => {
                    embassy_time::Timer::after_secs(1).await;
                }
            }
        }
    })
    .await;

    unreachable!()
}

async fn ble_runner_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if runner.run().await.is_err() {
            return;
        }
    }
}

async fn advertise_undirected<'v, 's, C: Controller>(
    name: &'v str,
    peripheral: &mut Peripheral<'v, C, DefaultPacketPool>,
    server: &'s HidGattServer<'v>,
) -> Result<GattConnection<'v, 's, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv_data = [0u8; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteServiceUuids16(&[[0x12, 0x18], [0x0f, 0x18]]),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv_data[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..len],
                scan_data: &[],
            },
        )
        .await?;
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}

async fn advertise_directed<'v, 's, C: Controller>(
    peer: Address,
    peripheral: &mut Peripheral<'v, C, DefaultPacketPool>,
    server: &'s HidGattServer<'v>,
) -> Result<GattConnection<'v, 's, DefaultPacketPool>, BleHostError<C::Error>> {
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableNonscannableDirected { peer },
        )
        .await?;
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}

async fn gatt_event_loop<'v, 's, P: PacketPool>(
    _server: &'s HidGattServer<'v>,
    conn: &GattConnection<'v, 's, P>,
) {
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { .. } => return,
            GattConnectionEvent::Gatt { event } => {
                if let Ok(reply) = event.accept() {
                    reply.send().await;
                }
            }
            GattConnectionEvent::PassKeyDisplay(_key) => {
                defmt::info!("[ble] passkey display requested (ignored for Just Works)");
            }
            GattConnectionEvent::PassKeyConfirm(_key) => {
                let _ = conn.pass_key_confirm();
            }
            GattConnectionEvent::PassKeyInput => {
                defmt::info!("[ble] passkey input requested (ignored for Just Works)");
            }
            GattConnectionEvent::PairingComplete { bond, .. } => {
                if bond.is_some() {
                    defmt::info!("[ble] pairing complete with bond");
                } else {
                    defmt::info!("[ble] pairing complete without bond");
                }
            }
            GattConnectionEvent::PairingFailed(_) => {
                defmt::warn!("[ble] pairing failed");
            }
            _ => {}
        }
    }
}
