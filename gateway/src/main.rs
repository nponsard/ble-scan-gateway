#![no_main]
#![no_std]
mod pins;
mod sensors;

use ariel_os::{
    debug::log::{error, info},
    hal, net,
    reexports::embassy_net,
    time::Timer,
    uart::Baud,
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

use crate::pins::Peripherals;

const MAX_CONCURRENT_CONNECTIONS: usize = 2;

const ENDPOINT_URL: &str = "http://10.42.0.1:3000/mac";

const TCP_BUFFER_SIZE: usize = 1024;
const HTTP_BUFFER_SIZE: usize = 1024;

#[ariel_os::task(autostart, peripherals)]
async fn main(peripherals: Peripherals) {
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

    loop {


        // read from UART
        // add to the buffer
        // search for 0x00 (COBS delimiter)
        // if found, decode the packet
        // save the data into the list of seen addresses
        // trim the buffer until the delimiter (remove the processed packet)



        info!("FIXME: Implement proper core sleep");
        Timer::after_secs(100).await;
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
