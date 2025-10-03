#![no_main]
#![no_std]

mod pins;

use embassy_futures::join::join;
use heapless::FnvIndexMap;
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
    debug::log::{info, trace, warn},
    thread::sync::Mutex,
    time::{Duration, Instant, Timer},
};

use common_types::{AddressesSeen, MAX_SEEN};

use embassy_nrf::peripherals::SERIAL0;
use embassy_nrf::{bind_interrupts, uarte};

static SEEN: Mutex<FnvIndexMap<BdAddr, Instant, MAX_SEEN>> = Mutex::new(FnvIndexMap::new());

bind_interrupts!(struct Irqs {
    SERIAL0 => uarte::InterruptHandler<SERIAL0>;
});

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
async fn send_scan_data(peripherals: pins::Peripherals) {
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;

    let mut uart = uarte::Uarte::new(
        peripherals.serial,
        Irqs,
        peripherals.uart_rx,
        peripherals.uart_tx,
        config,
    );
    loop {
        Timer::after_secs(2).await;
        info!("Sending scan data...");
        // Remove entries older than 10 minutes
        let seen = {
            let mut seen = SEEN.lock();
            let v: heapless::Vec<BdAddr, MAX_SEEN> = seen.keys().cloned().collect();
            seen.clear();
            v
        };
        let buffer = &mut [0u8; 1024];
        let data = serialize_with_flavor::<AddressesSeen, Cobs<Slice>, &mut [u8]>(
            &AddressesSeen::from(seen),
            Cobs::try_new(Slice::new(buffer)).unwrap(),
        );

        match data {
            Ok(slice) => match uart.write(slice).await {
                Ok(_) => {
                    info!("Sent {} bytes", slice.len());
                }
                Err(e) => {
                    warn!("Failed to send data over UART: {:?}", e);
                }
            },
            Err(e) => {
                warn!("Failed to serialize data: {}", e);
            }
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
                trace!("discovered: {:?}", report.addr);
                // force cleanup if we have too many entries
                if seen.len() >= MAX_SEEN {
                    remove_old_entries(&mut seen);
                    // if we still have too many entries, remove the oldest one
                    if seen.len() >= MAX_SEEN {
                        warn!("too many seen entries, removing oldest");
                        remove_oldest_entry(&mut seen);
                    }
                }
            }
            // Update / insert the address with the current time
            let _ = seen.insert(report.addr, Instant::now());
        }
    }
}
