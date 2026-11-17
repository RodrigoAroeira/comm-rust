mod http;
mod uart;
mod wifi;

use std::sync::{Arc, Mutex};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::link_patches;

fn main() -> anyhow::Result<()> {
    link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let wifi = wifi::start_ap(peripherals.modem, sys_loop, nvs)?;

    let uart = uart::open(
        peripherals.uart1,
        peripherals.pins.gpio17,
        peripherals.pins.gpio16,
    )?;
    let shared = Arc::new(Mutex::new(uart));

    let server = http::start(shared)?;

    // Dropping these would stop Wi-Fi and the HTTP server.
    let _keepalive = (wifi, server);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
