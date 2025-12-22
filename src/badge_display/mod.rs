pub mod display_image;

use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32},
};
use defmt::*;
use display_image::get_current_image;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_futures::select::select;
use embassy_rp::gpio;
use embassy_rp::gpio::Input;
use embassy_sync::{
    blocking_mutex::{
        self,
        raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    },
    signal::Signal,
};
use embassy_time::{Delay, Duration, Timer};
use embedded_graphics::{
    image::Image,
    mono_font::{MonoTextStyle, ascii::*},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::Text,
};
use embedded_text::{
    TextBox,
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
};
use gpio::Output;
use heapless::{String, Vec};
use tinybmp::Bmp;
use uc8151::LUT;
use uc8151::WIDTH;
use uc8151::{HEIGHT, asynch::Uc8151};
use {defmt_rtt as _, panic_probe as _};

use crate::{
    Spi0Bus,
    env::env_value,
    helpers::{easy_format, easy_format_str},
};

pub type RecentWifiNetworksVec = Vec<String<32>, 4>;

//Display state
pub static SCREEN_TO_SHOW: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Screen>> =
    blocking_mutex::Mutex::new(RefCell::new(Screen::Badge));
pub static RECENT_WIFI_NETWORKS: blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    RefCell<RecentWifiNetworksVec>,
> = blocking_mutex::Mutex::new(RefCell::new(RecentWifiNetworksVec::new()));

pub static DISPLAY_CHANGED: Signal<ThreadModeRawMutex, ()> = Signal::new();
pub static CURRENT_IMAGE: AtomicU8 = AtomicU8::new(0);
pub static CHANGE_IMAGE: AtomicBool = AtomicBool::new(true);
pub static WIFI_COUNT: AtomicU32 = AtomicU32::new(0);
pub static RTC_TIME_STRING: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<String<8>>> =
    blocking_mutex::Mutex::new(RefCell::new(String::<8>::new()));
pub static TEMP: AtomicU8 = AtomicU8::new(0);
pub static HUMIDITY: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum Screen {
    Badge,
    WifiList,
}

#[embassy_executor::task]
pub async fn run_the_display(
    spi_bus: &'static Spi0Bus,
    cs: Output<'static>,
    dc: Output<'static>,
    busy: Input<'static>,
    reset: Output<'static>,
) {
    let spi_dev = SpiDevice::new(&spi_bus, cs);
    let mut display = Uc8151::new(spi_dev, dc, busy, reset, Delay);

    // Note we're setting the Text color to `Off`. The driver is set up to treat Off as Black so that BMPs work as expected.
    let character_style = MonoTextStyle::new(&FONT_9X18_BOLD, BinaryColor::Off);
    let textbox_style = TextBoxStyleBuilder::new()
        .height_mode(HeightMode::FitToText)
        .alignment(HorizontalAlignment::Left)
        .paragraph_spacing(6)
        .build();

    // Bounding box for our text. Fill it with the opposite color so we can read the text.
    let name_and_detail_bounds = Rectangle::new(Point::new(0, 40), Size::new(WIDTH - 75, 0));
    name_and_detail_bounds
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(&mut display)
        .unwrap();
    info!("Name: {}", env_value("NAME"));
    info!("Details: {}", env_value("DETAILS"));
    let mut name_and_details_buffer = [0; 128];
    let name_and_details = easy_format_str(
        format_args!("{}\n{}", env_value("NAME"), env_value("DETAILS")),
        &mut name_and_details_buffer,
    );

    let name_and_detail_box = TextBox::with_textbox_style(
        &name_and_details.unwrap(),
        name_and_detail_bounds,
        character_style,
        textbox_style,
    );

    let mut current_screen = Screen::Badge;

    loop {
        select(Timer::after_secs(60), DISPLAY_CHANGED.wait()).await;

        display.enable();
        display.reset().await;
        let _ = display.setup(LUT::Medium).await;
        Timer::after_millis(50).await;

        let force_screen_refresh = true;

        SCREEN_TO_SHOW.lock(|x| current_screen = *x.borrow());
        // info!("Current Screen: {:?}", current_screen);
        if current_screen == Screen::Badge {
            if force_screen_refresh {
                // Draw the text box.
                name_and_detail_box.draw(&mut display).unwrap();
            }

            let count = WIFI_COUNT.load(core::sync::atomic::Ordering::Relaxed);
            info!("Wifi count: {}", count);
            let temp = TEMP.load(core::sync::atomic::Ordering::Relaxed);
            let humidity = HUMIDITY.load(core::sync::atomic::Ordering::Relaxed);
            let top_text: String<64> = easy_format::<64>(format_args!(
                "{}F {}% Wifi found: {}",
                temp, humidity, count
            ));
            let top_bounds = Rectangle::new(Point::new(0, 0), Size::new(WIDTH, 24));
            top_bounds
                .into_styled(
                    PrimitiveStyleBuilder::default()
                        .stroke_color(BinaryColor::Off)
                        .fill_color(BinaryColor::On)
                        .stroke_width(1)
                        .build(),
                )
                .draw(&mut display)
                .unwrap();

            Text::new(top_text.as_str(), Point::new(8, 16), character_style)
                .draw(&mut display)
                .unwrap();

            // Draw the text box.
            let result = display.partial_update(top_bounds.try_into().unwrap()).await;
            match result {
                Ok(_) => {}
                Err(_) => {
                    info!("Error updating display");
                }
            }

            let mut time_text: String<8> = String::<8>::new();

            let time_box_rectangle_location = Point::new(0, 96);
            RTC_TIME_STRING.lock(|x| {
                time_text.push_str(x.borrow().as_str()).unwrap();
            });

            //The bounds of the box for time and refresh area
            let time_bounds = Rectangle::new(time_box_rectangle_location, Size::new(88, 24));
            time_bounds
                .into_styled(
                    PrimitiveStyleBuilder::default()
                        .stroke_color(BinaryColor::Off)
                        .fill_color(BinaryColor::On)
                        .stroke_width(1)
                        .build(),
                )
                .draw(&mut display)
                .unwrap();

            //Adding a y offset to the box location to fit inside the box
            Text::new(
                time_text.as_str(),
                (
                    time_box_rectangle_location.x + 8,
                    time_box_rectangle_location.y + 16,
                )
                    .into(),
                character_style,
            )
            .draw(&mut display)
            .unwrap();

            let result = display
                .partial_update(time_bounds.try_into().unwrap())
                .await;
            match result {
                Ok(_) => {}
                Err(_) => {
                    info!("Error updating display");
                }
            }

            //Manually triggered display events

            if CHANGE_IMAGE.load(core::sync::atomic::Ordering::Relaxed) || force_screen_refresh {
                let current_image = get_current_image();
                let tga: Bmp<BinaryColor> = Bmp::from_slice(&current_image.image()).unwrap();
                let image = Image::new(&tga, current_image.image_location());
                //clear image location by writing a white rectangle over previous image location
                let clear_rectangle = Rectangle::new(
                    current_image.previous().image_location(),
                    Size::new(157, 101),
                );
                clear_rectangle
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(&mut display)
                    .unwrap();

                let _ = image.draw(&mut display);
                //TODO need to look up the reginal area display
                let _ = display.update().await;
                CHANGE_IMAGE.store(false, core::sync::atomic::Ordering::Relaxed);
            }
        } else {
            if force_screen_refresh {
                let top_bounds = Rectangle::new(Point::new(0, 0), Size::new(WIDTH, 24));
                top_bounds
                    .into_styled(
                        PrimitiveStyleBuilder::default()
                            .stroke_color(BinaryColor::Off)
                            .fill_color(BinaryColor::On)
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(&mut display)
                    .unwrap();

                let top_text: String<64> = easy_format::<64>(format_args!(
                    "Wifi found: {}",
                    WIFI_COUNT.load(core::sync::atomic::Ordering::Relaxed)
                ));

                Text::new(top_text.as_str(), Point::new(8, 16), character_style)
                    .draw(&mut display)
                    .unwrap();

                let result = display.partial_update(top_bounds.try_into().unwrap()).await;
                match result {
                    Ok(_) => {}
                    Err(_) => {
                        info!("Error updating display");
                    }
                }

                //write the wifi list
                let mut y_offset = 24;
                let wifi_list = RECENT_WIFI_NETWORKS.lock(|x| x.borrow().clone());
                for wifi in wifi_list.iter() {
                    // let wifi_text: String<64> = easy_format::<64>(format_args!("{}", wifi));
                    let wifi_bounds = Rectangle::new(Point::new(0, y_offset), Size::new(WIDTH, 24));
                    wifi_bounds
                        .into_styled(
                            PrimitiveStyleBuilder::default()
                                .stroke_color(BinaryColor::Off)
                                .fill_color(BinaryColor::On)
                                .stroke_width(1)
                                .build(),
                        )
                        .draw(&mut display)
                        .unwrap();

                    Text::new(wifi.trim(), Point::new(8, y_offset + 16), character_style)
                        .draw(&mut display)
                        .unwrap();

                    let result = display
                        .partial_update(wifi_bounds.try_into().unwrap())
                        .await;
                    match result {
                        Ok(_) => {}
                        Err(_) => {
                            info!("Error updating display");
                        }
                    }
                    y_offset += 24;
                }
            }
        }

        display.disable();
    }
}
