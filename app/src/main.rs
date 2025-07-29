#![no_main]
#![no_std]

use ariel_os::{
    debug::log::{error, info},
    net,
    reexports::embassy_net,
    time::Timer,
};
use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    client::HttpClient,
    headers::ContentType,
    request::{Method, RequestBuilder},
};

const MAX_SEEN: usize = 128;
const SHARED_MEMORY_SIZE: usize = MAX_SEEN * 6;
const SHARED_MEMORY_START: usize = 0x20080000 - SHARED_MEMORY_SIZE;

const TCP_BUFFER_SIZE: usize = 1024;
const HTTP_BUFFER_SIZE: usize = 1024;

const MAX_CONCURRENT_CONNECTIONS: usize = 2;

const ENDPOINT_URL: &str = "http://10.42.0.1:3000/mac";

#[ariel_os::task(autostart)]
async fn main() {
    let stack = net::network_stack().await.unwrap();

    let tcp_client_state =
        TcpClientState::<MAX_CONCURRENT_CONNECTIONS, TCP_BUFFER_SIZE, TCP_BUFFER_SIZE>::new();
    let tcp_client = TcpClient::new(stack, &tcp_client_state);
    let dns_client = DnsSocket::new(stack);

    let mut client = HttpClient::new(&tcp_client, &dns_client);

    let spu = embassy_nrf::pac::SPU;
    for i in 0..64 {
        spu.ramregion(i as usize).perm().write(|w| {
            w.set_execute(true);
            w.set_write(true);
            w.set_read(true);
            w.set_secattr(false);
            w.set_lock(false);
        })
    }

    info!("Starting BLE Scan Reporter Demo...");
    // start the network core
    embassy_nrf::reset::release_network_core();

    info!("Waiting for network configuration to be up...");
    stack.wait_config_up().await;

    loop {
        info!("loop");
        Timer::after_secs(5).await;

        info!("reading shared memory...");

        let body = {
            let seen_ptr = SHARED_MEMORY_START as *const u8;
            unsafe { core::slice::from_raw_parts(seen_ptr, SHARED_MEMORY_SIZE) }
        };

        info!(
            "Sending HTTP POST request to {} with body: {:?}",
            ENDPOINT_URL, body
        );

        if let Err(err) = send_http_post_request(&mut client, ENDPOINT_URL, b"test").await {
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
