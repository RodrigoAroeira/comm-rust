use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi,
};

const SSID: &str = "comm-gateway";
const PASSWORD: &str = "optma-lab";
const CHANNEL: u8 = 11;

pub fn start_ap(
    modem: impl WifiModemPeripheral + 'static,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;

    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: SSID.try_into()?,
        ssid_hidden: false,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into()?,
        channel: CHANNEL,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.wait_netif_up()?;

    log::info!("AP up: SSID={SSID} -> http://192.168.4.1");

    Ok(wifi)
}
