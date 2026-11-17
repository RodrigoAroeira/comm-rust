use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{AnyIOPin, InputPin, OutputPin};
use esp_idf_svc::hal::uart::{Uart, UartDriver, config::Config as UartConfig};
use protocol::{ACK_BYTE, TestCommand};

const ACK_TIMEOUT_MS: u64 = 250;

pub fn open(
    uart: impl Uart + 'static,
    tx: impl OutputPin + 'static,
    rx: impl InputPin + 'static,
) -> Result<UartDriver<'static>> {
    let config = UartConfig::new().baudrate(115_200.into());

    Ok(UartDriver::new(
        uart,
        tx,
        rx,
        None::<AnyIOPin>,
        None::<AnyIOPin>,
        &config,
    )?)
}

pub fn forward(uart: &Mutex<UartDriver<'static>>, cmd: &TestCommand) -> bool {
    let Ok(uart) = uart.lock() else {
        return false;
    };

    // Discard stale bytes before sending.
    if uart.clear_rx().is_err() {
        return false;
    }

    if !matches!(uart.write(&cmd.encode()), Ok(8)) {
        return false;
    }

    let timeout = TickType::from(Duration::from_millis(ACK_TIMEOUT_MS)).0;
    let mut byte = [0u8; 1];

    loop {
        match uart.read(&mut byte, timeout) {
            Ok(1) if byte[0] == ACK_BYTE => return true,
            Ok(1) => {}
            _ => return false,
        }
    }
}
