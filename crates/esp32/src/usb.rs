//! Line-oriented JSON REPL on the ESP console.
//!
//! By default the command bytes come from `io::stdin()` (the primary console:
//! UART0 on the S3, the only console on the classic ESP32). With the
//! `native-usb` feature on an ESP32-S3 they come from the USB Serial/JTAG
//! peripheral instead, using GPIO19 (D-) and GPIO20 (D+).
//!
//! Paste one command per line, e.g.
//!
//! ```text
//! {"test_type":"esr","cap_model":"A100F","pulse_ma":10000,"pulse_ms":1425}
//! ```
//!
//! The gateway answers with the same JSON status object as the HTTP endpoint.
//! `println!` and `log` keep going to stdout, which the ESP-IDF fans out to
//! both the primary console and (on the S3) the USB secondary console.

#[cfg(not(all(feature = "native-usb", esp32s3)))]
use std::io::{self, Read};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::uart::UartDriver;
#[cfg(all(feature = "native-usb", esp32s3))]
use esp_idf_svc::hal::usb_serial::UsbSerialDriver;
use protocol::TestCommand;

use crate::uart;

const MAX_LINE: usize = 512;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Uniform non-blocking byte source for the line parser.
trait CommandSource {
    /// One byte, or `None` when nothing is available right now.
    fn read_byte(&mut self) -> Result<Option<u8>>;
}

#[cfg(not(all(feature = "native-usb", esp32s3)))]
struct StdinSource {
    stdin: io::Stdin,
}

#[cfg(not(all(feature = "native-usb", esp32s3)))]
impl CommandSource for StdinSource {
    fn read_byte(&mut self) -> Result<Option<u8>> {
        let mut byte = [0u8; 1];
        match self.stdin.lock().read(&mut byte) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(byte[0])),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(all(feature = "native-usb", esp32s3))]
struct UsbSource(UsbSerialDriver<'static>);

#[cfg(all(feature = "native-usb", esp32s3))]
impl CommandSource for UsbSource {
    fn read_byte(&mut self) -> Result<Option<u8>> {
        let mut byte = [0u8; 1];
        // Zero timeout keeps this a poll, like the console stdin path.
        let read = self.0.read(&mut byte, 0)?;
        Ok(if read > 0 { Some(byte[0]) } else { None })
    }
}

#[cfg(not(all(feature = "native-usb", esp32s3)))]
pub fn run(shared: Arc<Mutex<UartDriver<'static>>>) -> Result<()> {
    repl_loop(shared, StdinSource { stdin: io::stdin() })
}

#[cfg(all(feature = "native-usb", esp32s3))]
pub fn run(shared: Arc<Mutex<UartDriver<'static>>>, usb: UsbSerialDriver<'static>) -> Result<()> {
    repl_loop(shared, UsbSource(usb))
}

fn repl_loop<S: CommandSource>(
    shared: Arc<Mutex<UartDriver<'static>>>,
    mut source: S,
) -> Result<()> {
    log::info!("Serial REPL ready: send one JSON command per line");

    let mut line = Vec::with_capacity(MAX_LINE);
    let mut overflow = false;

    loop {
        let byte = match source.read_byte()? {
            Some(byte) => byte,
            None => {
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
        };

        match byte {
            b'\n' => {
                if !overflow {
                    handle_line(&shared, &line);
                }
                line.clear();
                overflow = false;
            }
            b'\r' => {}
            _ if overflow => {}
            byte if line.len() < MAX_LINE => line.push(byte),
            _ => {
                line.clear();
                overflow = true;
                println!(r#"{{"status": "LINE_TOO_LONG"}}"#);
            }
        }
    }
}

fn handle_line(shared: &Arc<Mutex<UartDriver<'static>>>, line: &[u8]) {
    if line.is_empty() {
        return;
    }

    let status = std::str::from_utf8(line)
        .ok()
        .and_then(|text| TestCommand::from_json(text.trim()).ok())
        .map(|cmd| {
            if uart::forward(shared, &cmd) {
                "FORWARD_SUCCESS"
            } else {
                "FORWARD_FAIL"
            }
        })
        .unwrap_or("BAD_JSON");

    println!(r#"{{"status": "{status}"}}"#);
}
