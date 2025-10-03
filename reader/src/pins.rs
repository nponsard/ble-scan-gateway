use ariel_os::hal::peripherals;


#[cfg(context = "nrf52840dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_08,
    uart_rx: P0_06,
    serial: UARTE0,
});


#[cfg(context = "nrf52dk")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_08,
    uart_rx: P0_06,
    serial: UARTE0,
});


#[cfg(context = "nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_04,
    uart_rx: P0_05,
    serial: SERIAL3,
});
