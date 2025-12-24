use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};

use crate::{
    RtcDevice,
    state::{POWER_MUTEX, RTC_TIME},
};

pub async fn get_time(rtc_device: &'static Mutex<ThreadModeRawMutex, RtcDevice>) {
    let _guard = POWER_MUTEX.lock().await;
    let result = rtc_device.lock().await.get_datetime().await.ok();
    let mut data = RTC_TIME.lock().await;
    *data = result;
}
