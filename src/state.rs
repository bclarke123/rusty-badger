use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use serde::Deserialize;
use time::PrimitiveDateTime;

pub static POWER_MUTEX: Mutex<ThreadModeRawMutex, ()> = Mutex::new(());
pub static RTC_TIME: Mutex<ThreadModeRawMutex, Option<PrimitiveDateTime>> = Mutex::new(None);
pub static WEATHER: Mutex<ThreadModeRawMutex, Option<CurrentWeather>> = Mutex::new(None);

#[derive(Deserialize, Copy, Clone)]
pub struct CurrentWeather {
    pub temperature: f32,
    pub weathercode: u8,
    // pub is_day: u8,
}
