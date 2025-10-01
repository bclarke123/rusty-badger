use crate::I2c0Bus;
use crate::badge_display::{HUMIDITY, TEMP};
use defmt::*;
use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_time::Timer;
use shtcx::{self, PowerMode};

#[embassy_executor::task]
pub async fn run_the_temp_sensor(i2c_bus: &'static I2c0Bus) {
    let i2c_dev = I2cDevice::new(i2c_bus);

    let mut sht = shtcx::shtc3(i2c_dev);
    let mut sht_delay = embassy_time::Delay; // Create a delay instance

    loop {
        let combined = sht.measure(PowerMode::NormalMode, &mut sht_delay).unwrap();
        let celsius = combined.temperature.as_degrees_celsius();
        let fahrenheit = (celsius * 9.0 / 5.0) + 32.0;
        info!(
            "Temperature: {}°F, Humidity: {}%",
            fahrenheit,
            combined.humidity.as_percent()
        );
        TEMP.store(fahrenheit as u8, core::sync::atomic::Ordering::Relaxed);
        HUMIDITY.store(
            combined.humidity.as_percent() as u8,
            core::sync::atomic::Ordering::Relaxed,
        );
        Timer::after_secs(30).await;
    }
}
