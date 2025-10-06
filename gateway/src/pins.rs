use ariel_os::hal::peripherals;
// #[cfg(context = "nordic-thingy-91-x-nrf9151")]
// pub type Vcom0Uart<'a> = uart::SERIAL3<'a>;

// VCOM1
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_04,
    uart_rx: P0_05,
    btn1: P0_26,
    serial: SERIAL3,
    led: P0_30,
});
