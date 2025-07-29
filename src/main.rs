#![no_main]
#![no_std]

//! Adapted from the example in `trouble_host`
extern crate alloc;

use embassy_futures::join::join;
use heapless::FnvIndexMap;
use trouble_host::{
    Host,
    connection::{PhySet, ScanConfig},
    prelude::{BdAddr, EventHandler, LeAdvReportsIter},
    scan::Scanner,
};

use ariel_os::{
    debug::log::{error, info, warn},
    net,
    reexports::embassy_net,
    thread::sync::Mutex,
    time::{Duration, Instant, Timer},
};
use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    client::{HttpClient, TlsConfig, TlsVerify},
    headers::ContentType,
    request::{Method, RequestBuilder},
};

const MAX_SEEN: usize = 128;

static SEEN: Mutex<FnvIndexMap<BdAddr, Instant, MAX_SEEN>> = Mutex::new(FnvIndexMap::new());

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

/// Remove entries older than 10 minutes
fn remove_old_entries(seen: &mut FnvIndexMap<BdAddr, Instant, 128>) {
    let now = Instant::now();
    seen.retain(|_, &mut instant| now.duration_since(instant) < Duration::from_secs(600));
}

fn remove_oldest_entry(seen: &mut FnvIndexMap<BdAddr, Instant, 128>) {
    if let Some((oldest_key, _)) = seen.iter().min_by_key(|&(_, &v)| v) {
        seen.remove(&oldest_key.clone());
    }
}

#[ariel_os::task(autostart)]
async fn run_scanner() {
    info!("starting ble stack");

    let Host {
        central,
        mut runner,
        ..
    } = ariel_os::ble::ble_stack().await.build();

    let printer = DiscorveryHandler {};
    let mut scanner = Scanner::new(central);
    let _ = join(runner.run_with_handler(&printer), async {
        let config = ScanConfig::<'_> {
            active: true,
            phys: PhySet::M1,
            interval: Duration::from_secs(1),
            window: Duration::from_secs(1),
            ..Default::default()
        };
        let mut _session = scanner.scan(&config).await.unwrap();
        // Scan forever
        loop {
            info!("scanning...");
            Timer::after_secs(1).await;
        }
    })
    .await;
}

struct DiscorveryHandler {}

impl EventHandler for DiscorveryHandler {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        let mut seen = SEEN.lock();
        while let Some(Ok(report)) = it.next() {
            if !seen.contains_key(&report.addr) {
                info!("discovered: {:?}", report.addr);
                // force cleanup if we have too many entries
                if seen.len() >= MAX_SEEN {
                    remove_old_entries(&mut seen);
                    // if we still have too many entries, remove the oldest one
                    if seen.len() >= MAX_SEEN {
                        warn!("too many seen entries, removing oldest");
                        remove_oldest_entry(&mut seen);
                    }
                }
                seen.insert(report.addr, Instant::now()).unwrap();
            }
        }
    }
}

// RFC8449: TLS 1.3 encrypted records are limited to 16 KiB + 256 bytes.
const MAX_ENCRYPTED_TLS_13_RECORD_SIZE: usize = 16640;
// Required by `embedded_tls::TlsConnection::new()`.
const TLS_READ_BUFFER_SIZE: usize = MAX_ENCRYPTED_TLS_13_RECORD_SIZE;
// Can be smaller than the read buffer (could be adjusted: trade-off between memory usage and not
// splitting large writes into multiple records).
const TLS_WRITE_BUFFER_SIZE: usize = 4096;

const TCP_BUFFER_SIZE: usize = 1024;
const HTTP_BUFFER_SIZE: usize = 1024;

const MAX_CONCURRENT_CONNECTIONS: usize = 2;

const ENDPOINT_URL: &str = "https://httpbin.org/post";

#[ariel_os::task(autostart)]
async fn main() {
    let stack = net::network_stack().await.unwrap();

    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let tls_seed: u64 = rand_core::RngCore::next_u64(&mut ariel_os::random::crypto_rng());

    let mut tls_rx_buffer = [0; TLS_READ_BUFFER_SIZE];
    let mut tls_tx_buffer = [0; TLS_WRITE_BUFFER_SIZE];

    // We do not authenticate the server in this example, as that would require setting up a PSK
    // with the server.
    let tls_verify = TlsVerify::None;
    let tls_config = TlsConfig::new(tls_seed, &mut tls_rx_buffer, &mut tls_tx_buffer, tls_verify);

    let mut client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config);

    stack.wait_config_up().await;

    loop {
        Timer::after_secs(30).await;

        let value = {
            let seen = SEEN.lock();

            serde_json::value::to_value(
                seen.keys()
                    .map(|addr| addr.raw())
                    .collect::<alloc::vec::Vec<_>>(),
            )
            .unwrap()
        };
        let body = serde_json::to_vec(&value).unwrap_or_default();

        if let Err(err) = send_http_post_request(&mut client, ENDPOINT_URL, &body).await {
            error!(
                "Error while sending an HTTP request: {:?}",
                defmt::Debug2Format(&err)
            );
        }
    }
}

async fn send_http_post_request(
    client: &mut HttpClient<'_, TcpClient<'_, MAX_CONCURRENT_CONNECTIONS>, DnsSocket<'_>>,
    url: &str,
    content: &[u8],
) -> Result<(), reqwless::Error> {
    let mut http_rx_buf = [0; HTTP_BUFFER_SIZE];

    let handle = client.request(Method::POST, url).await?;
    let mut handle = handle
        .body(content)
        .content_type(ContentType::ApplicationJson);
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
