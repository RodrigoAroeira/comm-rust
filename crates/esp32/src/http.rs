use std::sync::{Arc, Mutex};

use embedded_svc::http::Headers;
use esp_idf_svc::hal::uart::UartDriver;
use esp_idf_svc::http::server::{Configuration as ServerConfig, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::{Read, Write};
use protocol::TestCommand;

use crate::uart;

const MAX_BODY: usize = 256;
const RESP_OK: &[u8] = br#"{"status": "FORWARD_SUCCESS"}"#;
const RESP_FAIL: &[u8] = br#"{"status": "FORWARD_FAIL"}"#;

pub fn start(shared: Arc<Mutex<UartDriver<'static>>>) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&ServerConfig {
        // JSON parsing needs more stack than the default handler stack.
        stack_size: 10240,
        ..Default::default()
    })?;

    server.fn_handler::<anyhow::Error, _>("/api/run-test", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;

        if len == 0 || len > MAX_BODY {
            req.into_status_response(413)?.write_all(RESP_FAIL)?;
            return Ok(());
        }

        let mut body = vec![0u8; len];
        req.read_exact(&mut body)?;

        let ok = std::str::from_utf8(&body)
            .ok()
            .and_then(|text| TestCommand::from_json(text).ok())
            .map(|cmd| uart::forward(&shared, &cmd))
            .unwrap_or(false);

        let payload = if ok { RESP_OK } else { RESP_FAIL };
        req.into_ok_response()?.write_all(payload)?;

        Ok(())
    })?;

    log::info!("HTTP listening on port 80");

    Ok(server)
}
