#![no_main]
#![no_std]
mod pins;
mod sensors;

use ariel_os::{
    asynch::Spawner,
    debug::log::{error, info, warn},
    gpio::{Input, Level, Output, Pull},
    hal, net,
    reexports::embassy_net,
    sensors::{Label, Reading, Sensor},
    thread::sync::Mutex,
    time::{Duration, Instant, Timer},
    uart::Baud,
};
use ariel_os_nrf91_gnss::Nrf91GnssExt;
use common_types::{AddressesSeen, GatewayUpdate, Location, MAX_SEEN};
use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embedded_io_async::Read as _;
use heapless::{FnvIndexMap, Vec};
use reqwless::{
    client::HttpClient,
    headers::ContentType,
    request::{Method, RequestBuilder},
};

use crate::{pins::Peripherals, sensors::NRF91_GNSS};

type SeenMap = FnvIndexMap<[u8; 6], Instant, MAX_SEEN>;
static SEEN: Mutex<SeenMap> = Mutex::new(FnvIndexMap::new());

static CURRENT_LOCATION: Mutex<Option<Location>> = Mutex::new(None);

const MAX_CONCURRENT_CONNECTIONS: usize = 2;

const ENDPOINT_URL: &str = "http://10.42.0.1:3000/mac";

const TCP_BUFFER_SIZE: usize = 1024;
const HTTP_BUFFER_SIZE: usize = 1024;

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
            let mut seen = SEEN.lock();
            remove_old_entries(&mut seen);
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn uart_receive(peripherals: Peripherals) {
    let mut config = hal::uart::Config::default();
    config.baudrate = Baud::_115200;
    info!("Selected configuration: {}", config);

    let mut rx_buf = [0u8; 32];
    let mut tx_buf = [0u8; 32];

    let mut uart = pins::Vcom0Uart::new(
        peripherals.uart_rx,
        peripherals.uart_tx,
        &mut rx_buf,
        &mut tx_buf,
        config,
    );

    let mut packet_buffer: Vec<u8, 2048> = Vec::new();
    let mut uart_read_buf = [0u8; 64];

    loop {
        let read = uart.read(&mut uart_read_buf).await.unwrap();
        packet_buffer
            .extend_from_slice(&uart_read_buf[..read])
            .unwrap();
        if let Some(separator) = packet_buffer.iter().position(|&b| b == 0x00) {
            let instant = Instant::now();
            let packet = &mut packet_buffer[..separator];

            match postcard::from_bytes_cobs::<AddressesSeen>(packet) {
                Ok(decoded) => {
                    let mut seen = SEEN.lock();
                    for addr in decoded.addrs {
                        if seen.insert(addr, instant).is_err() {
                            warn!("Seen list full, removing oldest entry to insert new one");
                            remove_oldest_entry(&mut seen);
                            if seen.insert(addr, instant).is_err() {
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

#[ariel_os::task(autostart)]
async fn update_location() {
    let spawner = Spawner::for_current_executor().await;

    sensors::NRF91_GNSS
        .init(ariel_os_nrf91_gnss::config::Config::default())
        .await;
    spawner.spawn(sensors::nrf91_gnss_runner()).unwrap();

    loop {
        if let Err(e) = sensors::NRF91_GNSS.trigger_measurement() {
            warn!("Failed to trigger GNSS measurement: {:?}", e);
        }
        let reading = sensors::NRF91_GNSS.wait_for_reading().await;

        if let Ok(samples) = reading {
            let mut location = Location {
                altitude: 0.0,
                latitude: 0.0,
                longitude: 0.0,
                timestamp: 0,
            };
            let mut found_altitude = false;
            let mut found_latitude = false;
            let mut found_longitude = false;

            let found_timestamp = match samples.time_of_fix() {
                Ok(t) => {
                    location.timestamp = t as u64;
                    true
                }
                Err(e) => {
                    warn!("Failed to get time of fix: {:?}", e);
                    false
                }
            };

            for (sample, channel) in samples.samples().zip(NRF91_GNSS.reading_channels().iter()) {
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
                let mut loc_lock = CURRENT_LOCATION.lock();
                *loc_lock = Some(location);
            }
        }
    }
}

#[ariel_os::task(autostart, peripherals)]
async fn send_updates(peripherals: Peripherals) {
    let mut last_update = Instant::now();

    let stack = net::network_stack().await.unwrap();

    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let mut client = HttpClient::new(&tcp_client, &dns_client);

    let mut btn1 = Input::builder(peripherals.btn1, Pull::Up)
        .build_with_interrupt()
        .unwrap();

    loop {
        // Wait for the button being pressed or 60s, whichever comes first.
        let _ = embassy_futures::select::select(btn1.wait_for_low(), Timer::after_nanos(60)).await;

        // Prevent sending updates too frequently
        if last_update.elapsed() < Duration::from_secs(10) {
            warn!("Update skipped to avoid sending updates too frequently");
            continue;
        }

        let location = { *CURRENT_LOCATION.lock() };

        let seen_snapshot = { SEEN.lock().keys().cloned().collect() };

        let update = GatewayUpdate {
            location,
            seen: seen_snapshot,
        };
        match serde_json::to_vec(&update) {
            Ok(json) => {
                if let Err(e) = send_http_post_request(&mut client, ENDPOINT_URL, &json).await {
                    warn!(
                        "Failed to send HTTP POST request: {:?}",
                        defmt::Debug2Format(&e)
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to serialize update to JSON: {:?}",
                    defmt::Debug2Format(&e)
                );
            }
        }
        last_update = Instant::now();
    }
}

async fn send_http_post_request(
    client: &mut HttpClient<'_, TcpClient<'_, MAX_CONCURRENT_CONNECTIONS>, DnsSocket<'_>>,
    url: &str,
    content: &[u8],
) -> Result<(), reqwless::Error> {
    let mut http_rx_buf = [0; HTTP_BUFFER_SIZE];

    let handle: reqwless::client::HttpRequestHandle<
        '_,
        embassy_net::tcp::client::TcpConnection<'_, 2, 1024, 1024>,
        (),
    > = client.request(Method::POST, url).await?;
    let mut handle = handle
        .body(content)
        .content_type(ContentType::ApplicationOctetStream);
    let response = handle.send(&mut http_rx_buf).await?;

    info!("Response status: {}", response.status.0);

    if let Some(ref content_type) = response.content_type {
        info!("Response Content-Type: {}", content_type.as_str());
    }

    if let Ok(body) = response.body().read_to_end().await {
        if let Ok(body) = core::str::from_utf8(body) {
            info!("Response body:\n{}", body);
        } else {
            info!("Received a response body, but it is not valid UTF-8");
        }
    } else {
        info!("No response body");
    }

    Ok(())
}
