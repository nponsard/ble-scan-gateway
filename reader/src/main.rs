#![no_main]
#![no_std]

mod pins;

use embassy_futures::join::join;
use heapless::{FnvIndexMap, Vec};
use postcard::{
    ser_flavors::{Cobs, Slice},
    serialize_with_flavor,
};
use trouble_host::{
    Host,
    connection::{PhySet, ScanConfig},
    prelude::{BdAddr, EventHandler, LeAdvReportsIter},
    scan::Scanner,
};

use ariel_os::{
    debug::log::{debug, error, info, trace, warn},
    time::{Duration, Instant, Timer},
};

use common_types::{AddressesSeen, MAX_SEEN};
#[cfg(context = "nrf9151")]
use embassy_nrf::peripherals::SERIAL3;
#[cfg(any(context = "nrf52840", context = "nrf52832"))]
use embassy_nrf::peripherals::UARTE0;
use embassy_nrf::{bind_interrupts, uarte};

#[cfg(context = "nrf9151")]
bind_interrupts!(struct Irqs {
    SERIAL3 => uarte::InterruptHandler<SERIAL3>;
});

#[cfg(any(context = "nrf52840", context = "nrf52832"))]
bind_interrupts!(struct Irqs {
    UARTE0 => uarte::InterruptHandler<UARTE0>;
});

#[ariel_os::task(autostart, peripherals)]
async fn get_scan_data(peripherals: pins::Peripherals) {
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    let mut uart = uarte::Uarte::new(
        peripherals.serial,
        Irqs,
        peripherals.uart_tx,
        peripherals.uart_rx,
        config,
    );
    let mut packet_buffer: Vec<u8, 2048> = Vec::new();
    let mut uart_read_buf = [0u8; 128];

    // let mut buf = [0; 8];
    // buf.copy_from_slice(b"Hello!\r\n");

    // uart.write(&buf).await.unwrap();
    // info!("wrote hello in uart!");

    // loop {
    //     info!("reading...");
    //     let res = uart.read(&mut buf).await;
    //     if let Err(e) = res {
    //         error!("UART read error: {:?}", e);
    //         continue;
    //     } else {
    //         info!("read ok");
    //         info!("read: {:?}", &buf);
    //     }
    //     info!("writing...");
    //     uart.write(&buf).await.unwrap();
    // }

    loop {
        debug!("Waiting for UART data...");
        let result = uart.read(&mut uart_read_buf).await;
        if let Err(e) = result {
            error!("UART read error: {:?}", e);
            continue;
        }

        debug!("Read 64 bytes from UART");
        packet_buffer.extend_from_slice(&uart_read_buf).unwrap();
        if let Some(separator) = packet_buffer.iter().position(|&b| b == 0x00) {
            let instant = Instant::now();
            let packet = &mut packet_buffer[..separator];
            debug!("Received packet, trying to decode...");

            match postcard::from_bytes_cobs::<AddressesSeen>(packet) {
                Ok(decoded) => {
                    debug!("Decoded packet");
                    info!("Packet: {:?}", decoded.addrs);
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
