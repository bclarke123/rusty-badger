use cyw43::{Control, JoinOptions};
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use log::info;

use crate::state::POWER_MUTEX;

pub static FW: &[u8] = include_bytes!("../cyw43-firmware/43439A0.bin");
pub static CLM: &[u8] = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

static WIFI_SSID: &str = env!("WIFI_SSID");
static WIFI_PASSWORD: &[u8] = include_bytes!("../.wifi");

pub async fn connect(control: &mut Control<'_>, stack: &Stack<'_>) -> Result<(), ()> {
    let _guard = POWER_MUTEX.lock().await;

    let mut connected_to_wifi = false;

    for _ in 0..30 {
        match control
            .join(WIFI_SSID, JoinOptions::new(WIFI_PASSWORD))
            .await
        {
            Ok(_) => {
                connected_to_wifi = true;
                info!("join successful");
                break;
            }
            Err(err) => {
                info!("join failed with status={}", err.status);
            }
        }
        Timer::after(Duration::from_secs(1)).await;
    }

    if !connected_to_wifi {
        return Err(());
    }

    stack.wait_config_up().await;

    Ok(())
}
