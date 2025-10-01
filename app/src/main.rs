#![no_main]
#![no_std]

use ariel_os::{debug::log::info, time::Timer};

#[ariel_os::task(autostart)]
async fn main() {
    embassy_nrf::reset::hold_network_core();

    info!("Starting BLE Scan Reporter Demo...");
    // start the network core
    embassy_nrf::reset::release_network_core();

    loop {
        info!("FIXME: Implement proper core sleep");
        Timer::after_secs(100).await;
    }
}
