use ariel_os::hal::peripherals;

// VCOM0
#[cfg(context = "nrf5340dk-net")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_29,
    uart_rx: P1_04,
    serial: SERIAL0,
});
