use embassy_rp::flash::{Async, Flash};
use embassy_rp::peripherals::FLASH;
use embedded_storage_async::nor_flash::NorFlash;

use crate::FlashDevice;
use crate::state::CurrentWeather;

// The type signature for Async Flash (size is 2MB = 2097152)
pub type FlashDriver = Flash<'static, FLASH, Async, 2097152>;

// Define Flash Constants
const FLASH_OFFSET: u32 = 0x200000 - 0x1000; // Top of 2MB
const FLASH_SIZE: u32 = 4096;

pub async fn save_state(flash: &'static FlashDevice, state: &CurrentWeather) {
    // 1. Serialize to RAM
    let mut buf = [0u8; 128];
    let slice = match postcard::to_slice(state, &mut buf) {
        Ok(s) => s,
        Err(_) => {
            defmt::error!("Serialization failed - buffer too small?");
            return;
        }
    };

    let mut flash = flash.lock().await;

    // 2. Erase & Write
    let _ = flash.erase(FLASH_OFFSET, FLASH_OFFSET + FLASH_SIZE).await;
    let _ = flash.write(FLASH_OFFSET, slice).await;
}

pub async fn load_state(flash: &'static FlashDevice) -> Option<CurrentWeather> {
    let mut buf = [0u8; 128];

    // 1. Read (Async - uses DMA)
    if flash
        .lock()
        .await
        .read(FLASH_OFFSET, &mut buf)
        .await
        .is_err()
    {
        return None;
    }

    // 2. Deserialize (Sync)
    postcard::from_bytes(&buf).ok()
}
