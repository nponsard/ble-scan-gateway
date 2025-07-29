#![no_main]
#![no_std]

//! Adapted from the example in `trouble_host`

use embassy_futures::join::join;
use heapless::FnvIndexMap;
use trouble_host::{
    Host,
    connection::{PhySet, ScanConfig},
    prelude::{BdAddr, EventHandler, LeAdvReportsIter},
    scan::Scanner,
};

use ariel_os::{
    debug::log::{info, warn},
    thread::sync::Mutex,
    time::{Duration, Instant, Timer},
};

const MAX_SEEN: usize = 128;
const SHARED_MEMORY_SIZE: usize = MAX_SEEN * 6;
const SHARED_MEMORY_START: usize = 0x20080000 - SHARED_MEMORY_SIZE;

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

#[ariel_os::task(autostart)]
async fn update_shared_memory() {
    loop {
        Timer::after_secs(30).await;
        // Remove entries older than 10 minutes
        let seen = { SEEN.lock().clone() };
        let seen_raw = unsafe { &mut *(SHARED_MEMORY_START as *mut [u8; SHARED_MEMORY_SIZE]) };

        let mut index = 0;
        for (addr, _) in seen.iter() {
            if index >= SHARED_MEMORY_SIZE {
                break;
            }
            // Store the address in the shared memory
            for byte in addr.raw() {
                seen_raw[index] = *byte;
                index += 1;
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
