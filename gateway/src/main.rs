#![no_main]
#![no_std]
mod pins;
mod sensors;

use ariel_os::{
    debug::log::{error, info, warn},
    hal, net,
    reexports::embassy_net,
    thread::sync::Mutex,
    time::{Duration, Instant, Timer},
    uart::Baud,
};
use common_types::{AddressesSeen, MAX_SEEN};
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

use crate::pins::Peripherals;

type SeenMap = FnvIndexMap<[u8; 6], Instant, MAX_SEEN>;
static SEEN: Mutex<SeenMap> = Mutex::new(FnvIndexMap::new());

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
    let stack = net::network_stack().await.unwrap();

    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let mut client = HttpClient::new(&tcp_client, &dns_client);
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
