use ariel_os::hal::{peripherals, uart};
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
pub type Vcom0Uart<'a> = uart::SERIAL3<'a>;

// VCOM0
#[cfg(context = "nordic-thingy-91-x-nrf9151")]
ariel_os::hal::define_peripherals!(Peripherals {
    uart_tx: P0_00,
    uart_rx: P0_01,
});