#![no_main]
#![no_std]
mod pins;
mod sensors;

use core::{cell::RefCell, str::FromStr};

use coap_handler::Attribute;
use coap_handler_implementations::{GetRenderable, TypeHandler, wkc::ConstantSingleRecordReport};
use coap_request::Stack;
use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    mutex::Mutex,
};
use embedded_io_async::BufRead;
use heapless::{String, Vec, index_map::FnvIndexMap};

use ariel_os::{
    asynch::Spawner,
    debug::log::{Debug2Format, debug, error, info, warn},
    gpio::{Input, Level, Output, Pull},
    hal,
    sensors::{Label, Reading, Sensor},
    time::{Duration, Instant, Timer},
    uart::Baudrate,
};
use ariel_os_sensors_gnss_time_ext::GnssTimeExt as _;

use common_types::{AddressesSeen, DetectedTag, GatewayUpdate, Location, MAX_SEEN};

use crate::pins::{GnssStatusPeripherals, UartPeripherals, UpdatePeripherals};

type SeenMap = FnvIndexMap<String<64>, Instant, MAX_SEEN>;

static SEEN: Mutex<CriticalSectionRawMutex, SeenMap> = Mutex::new(FnvIndexMap::new());
static CURRENT_LOCATION: Mutex<CriticalSectionRawMutex, Option<Location>> = Mutex::new(None);

static LAST_UPDATE: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Option<GatewayUpdate>>> =
    blocking_mutex::Mutex::new(RefCell::new(None));

/// Remove entries older than 10 minutes
fn remove_old_entries(seen: &mut SeenMap) {
    let now = Instant::now();
    seen.retain(|_, &mut instant| now.duration_since(instant) < Duration::from_secs(600));
}

fn remove_oldest_entry(seen: &mut SeenMap) {
    if let Some((oldest_key, _)) = seen.iter().min_by_key(|&(_, &v)| v) {
        seen.remove(&oldest_key.clone());
    }
}

#[ariel_os::task(autostart)]
async fn automatic_cleanup() {
    loop {
        Timer::after_secs(30).await;
        // Remove entries older than 10 minutes
        {
            debug!("Cleaning up old entries in seen list");
            let mut seen = SEEN.lock().await;
            debug!("locked");
            remove_old_entries(&mut seen);
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn uart_receive(peripherals: UartPeripherals) {
    let mut config = hal::uart::Config::default();
    config.baudrate = Baudrate::_115200;

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 1];

    let mut uart = pins::ReceiverUart::new(
        peripherals.uart_rx,
        peripherals.uart_tx,
        &mut rx_buf,
        &mut tx_buf,
        config,
    )
    .expect("Invalid UART configuration");
    let mut packet_buffer: Vec<u8, 8192> = Vec::new();

    loop {
        debug!("Waiting for UART data...");
        let result = uart.fill_buf().await;
        let read = match result {
            Err(e) => {
                error!("UART read error: {:?}", e);
                continue;
            }
            Ok(n) => n,
        };
        let size_read = read.len();
        debug!("Read {} bytes from UART", size_read);
        let err = packet_buffer.extend_from_slice(read);
        if let Err(e) = err {
            warn!("Packet buffer full, dropping data: {:?}", Debug2Format(&e));
            packet_buffer.clear();
            continue;
        }
        uart.consume(size_read);

        if let Some(separator) = packet_buffer.iter().position(|&b| b == 0x00) {
            let instant = Instant::now();
            let packet = &mut packet_buffer[..separator];
            debug!("Received packet, trying to decode...");

            match postcard::from_bytes_cobs::<AddressesSeen>(packet) {
                Ok(decoded) => {
                    debug!("Decoded packet");
                    let mut seen = SEEN.lock().await;
                    for addr in decoded.addrs {
                        if seen.insert(addr.id.clone(), instant).is_err() {
                            warn!("Seen list full, removing oldest entry to insert new one");
                            remove_oldest_entry(&mut seen);
                            if seen.insert(addr.id, instant).is_err() {
                                error!("Failed to insert address after removing oldest entry");
                            }
                        };
                    }
                }
                Err(e) => {
                    warn!("Failed to decode packet: {:?}", e);
                }
            }

            // Remove the read buffer
            // should not panic as the size is smaller than the capacity of the vec.
            packet_buffer = Vec::from_slice(packet_buffer.split_at(separator + 1).1).unwrap();
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn update_location(peripherals: GnssStatusPeripherals) {
    let mut led_blue = Output::new(peripherals.led_blue, Level::Low);
    let mut led_red = Output::new(peripherals.led_red, Level::Low);
    led_red.set_high();

    let spawner = unsafe { Spawner::for_current_executor().await };
    unsafe {
        nrfxlib_sys::nrf_modem_gnss_prio_mode_enable();
    }

    // let res = nrf_modem::send_at::<0>(r#"AT+CPSMS=1,"","","00101000","00001000""#).await;
    // debug!("AT+CPSMS=1 result: {:?}", defmt::Debug2Format(&res));
    sensors::NRF91_GNSS
        .init(ariel_os_nrf91_gnss::config::Config::default())
        .await;
    spawner.spawn(sensors::nrf91_gnss_runner()).unwrap();

    loop {
        if let Err(e) = sensors::NRF91_GNSS.trigger_measurement() {
            warn!("Failed to trigger GNSS measurement: {:?}", e);
        }
        let reading = sensors::NRF91_GNSS.wait_for_reading().await;

        debug!("Got GNSS reading: {:?}", defmt::Debug2Format(&reading));

        if let Ok(samples) = reading {
            let mut location = Location {
                altitude: 0.0,
                latitude: 0.0,
                longitude: 0.0,
                time_of_fix: 0,

                // TODO: populate these values
                heading: 0.0,
                horizontal_speed: 0.0,
                vertical_spedd: 0.0,
            };
            let mut found_altitude = false;
            let mut found_latitude = false;
            let mut found_longitude = false;

            let found_timestamp = match samples.time_of_fix_timestamp() {
                Ok(t) => {
                    location.time_of_fix = t as u64;
                    true
                }
                Err(e) => {
                    warn!("Failed to get time of fix: {:?}", e);
                    false
                }
            };

            for (channel, sample) in samples.samples() {
                match channel.label() {
                    Label::Altitude => {
                        if let Ok(value) = sample.value() {
                            location.altitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_altitude = true;
                        }
                    }
                    Label::Latitude => {
                        if let Ok(value) = sample.value() {
                            location.latitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_latitude = true;
                        }
                    }
                    Label::Longitude => {
                        if let Ok(value) = sample.value() {
                            location.longitude =
                                value as f32 / 10i32.pow((-channel.scaling()) as u32) as f32;
                            found_longitude = true;
                        }
                    }
                    _ => {}
                }
            }

            if found_altitude && found_latitude && found_longitude && found_timestamp {
                led_red.set_low();
                led_blue.set_high();
                debug!("updating location");
                let mut loc_lock = CURRENT_LOCATION.lock().await;
                *loc_lock = Some(location);
            } else {
                led_blue.set_low();
            }
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn updates(peripherals: UpdatePeripherals) {
    let mut led = Output::new(peripherals.led_green, Level::Low);
    let mut btn1 = Input::builder(peripherals.btn1, Pull::Up)
        .build_with_interrupt()
        .unwrap();
    let mut last_update = Instant::now();

    loop {
        // Wait for the button being pressed or 60s, whichever comes first.
        info!("Waiting 60s before sending next update...");
        // unsafe {
        //     nrfxlib_sys::nrf_modem_gnss_prio_mode_enable();
        // }
        led.set_low();
        let _ = embassy_futures::select::select(btn1.wait_for_low(), Timer::after_secs(60)).await;
        led.set_high();
        // Timer::after_secs(60).await;
        // Prevent sending updates too frequently
        if last_update.elapsed() < Duration::from_secs(10) {
            warn!("Update skipped to avoid sending updates too frequently");
            continue;
        }

        info!("Sending update...");
        let location = { *CURRENT_LOCATION.lock().await };
        debug!("Getting seen list");
        let seen_snapshot: Vec<String<64>, 32> = { SEEN.lock().await.keys().cloned().collect() };

        // TODO : get the age and rssi of devices
        let detected_tags = seen_snapshot
            .iter()
            .map(|k| DetectedTag {
                age: 0,
                id: k.clone(),
                rssi: 0,
            })
            .collect();

        let update = GatewayUpdate {
            location,
            detected_tags,

            // TODO: get battery level
            battery_level: None,
            // TODO: configure gateway id
            gateway_id: String::from_str("test").unwrap(),
            // TODO: get time
            timestamp: 0,
        };

        // replace the last update
        let _ = LAST_UPDATE.lock(|s| s.borrow_mut().replace(update));

        // match serde_json::to_vec(&update) {
        //     Ok(json) => {

        //     }
        //     Err(e) => {
        //         warn!(
        //             "Failed to serialize update to JSON: {:?}",
        //             defmt::Debug2Format(&e)
        //         );
        //     }
        // }
        last_update = Instant::now();
    }
}

#[ariel_os::task(autostart)]
async fn register_to_rd() {
    let client = ariel_os::coap::coap_client().await;

    // Corresponding to the fixed network setup, we select a fixed server address; this may need to
    // be updated on hosts that are configured differently.
    let addr = "65.108.193.50:4230"; // IPv4 🔔
    let demoserver = addr.parse().unwrap();

    loop {
        info!("Sending POST to {}...", demoserver);
        let request = coap_request_implementations::Code::post()
            .with_path("/rd")
            .with_request_payload_slice(b"This is Ariel OS")
            .processing_response_payload_through(|p| {
                info!(
                    "RD response is {:?}",
                    core::str::from_utf8(p).map_err(|_| "not Unicode?")
                );
            });
        let response = client.to(demoserver).request(request).await;
        info!("Response {:?}", response.map_err(|_| "TransportError"));

        Timer::after_secs(60).await
    }
}

#[ariel_os::task(autostart)]
async fn coap_run() {
    use coap_handler_implementations::{HandlerBuilder, SimpleRendered, new_dispatcher};

    let handler = new_dispatcher()
        // We offer a single resource: /hello, which responds just with a text string.
        .at(&["hello"], SimpleRendered("Hello from Ariel OS"))
        .at(
            &["status"],
            ConstantSingleRecordReport::new(
                TypeHandler::new_minicbor_2(coap_handler_implementations::with_get(
                    StatusRenderer::new(),
                )),
                &[Attribute::Title("Gateway Status")],
            ),
        );

    ariel_os::coap::coap_run(handler).await;
}

struct StatusRenderer {}

impl StatusRenderer {
    pub fn new() -> StatusRenderer {
        StatusRenderer {}
    }
}

impl GetRenderable for StatusRenderer {
    type Get = GatewayUpdate;
    fn get(&mut self) -> Result<Self::Get, coap_message_utils::Error> {
        info!("GET /status");

        LAST_UPDATE
            .lock(|s| s.clone())
            .into_inner()
            .ok_or(coap_message_utils::Error::service_unavailable())
    }
}
