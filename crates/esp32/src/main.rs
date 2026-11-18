mod http;
mod uart;
mod usb;
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

    let server = http::start(shared.clone())?;

    #[cfg(all(feature = "native-usb", esp32s3))]
    let usb = {
        use esp_idf_svc::hal::usb_serial::{UsbSerialConfig, UsbSerialDriver};
        UsbSerialDriver::new(
            peripherals.usb_serial,
            peripherals.pins.gpio19,
            peripherals.pins.gpio20,
            // The REPL rejects lines over 512 bytes only after it has seen the
            // whole line; the default 256-byte RX ring would drop the tail.
            &UsbSerialConfig::new().rx_buffer_size(1024),
        )?
    };

    let repl = std::thread::Builder::new()
        .name("serial-repl".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            #[cfg(all(feature = "native-usb", esp32s3))]
            let result = usb::run(shared, usb);
            #[cfg(not(all(feature = "native-usb", esp32s3)))]
            let result = usb::run(shared);

            if let Err(error) = result {
                log::error!("serial REPL stopped: {error:?}");
            }
        })?;

    // Dropping these would stop Wi-Fi and the HTTP server.
    let _keepalive = (wifi, server, repl);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
