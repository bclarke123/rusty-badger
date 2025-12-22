pub mod display_image;

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU8, AtomicU32},
};
use defmt::*;
use display_image::get_current_image;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice as AsyncSpiDevice;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio;
use embassy_rp::gpio::Input;
use embassy_sync::{
    blocking_mutex::{
        self,
        raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    },
    signal::Signal,
};
use embassy_time::{Delay, Timer};
use embedded_graphics::{
    image::Image,
    mono_font::{MonoTextStyle, ascii::*},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::Text,
};
use embedded_hal_async::spi::SpiDevice;
use embedded_text::{
    TextBox,
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
};
use gpio::Output;
use heapless::{String, Vec};
use tinybmp::Bmp;
use uc8151::{LUT, WIDTH, asynch::Uc8151};
use {defmt_rtt as _, panic_probe as _};

use crate::{
    Spi0Bus,
    env::env_value,
    helpers::{easy_format, easy_format_str},
};

pub type RecentWifiNetworksVec = Vec<String<32>, 4>;

pub static RECENT_WIFI_NETWORKS: blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    RefCell<RecentWifiNetworksVec>,
> = blocking_mutex::Mutex::new(RefCell::new(RecentWifiNetworksVec::new()));

pub static CURRENT_IMAGE: AtomicU8 = AtomicU8::new(0);
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

pub static DISPLAY_CHANGED: Signal<ThreadModeRawMutex, Screen> = Signal::new();

#[embassy_executor::task]
pub async fn run_the_display(
    spi_bus: &'static Spi0Bus,
    cs: Output<'static>,
    dc: Output<'static>,
    busy: Input<'static>,
    reset: Output<'static>,
) {
    let spi_dev = AsyncSpiDevice::new(&spi_bus, cs);
    let mut display = Uc8151::new(spi_dev, dc, busy, reset, Delay);

    info!("Name: {}", env_value("NAME"));
    info!("Details: {}", env_value("DETAILS"));

    let mut current_screen = Screen::Badge;

    loop {
        let result = select(DISPLAY_CHANGED.wait(), Timer::after_secs(60)).await;

        if let Either::First(new_screen) = result {
            current_screen = new_screen;
        }

        display.enable();
        display.reset().await;
        let _ = display.setup(LUT::Medium).await;
        Timer::after_millis(50).await;

        match current_screen {
            Screen::Badge => {
                draw_badge(&mut display).await;
            }
            Screen::WifiList => {
                draw_wifi(&mut display).await;
            }
        }

        display.disable();
    }
}

async fn draw_badge<SPI>(
    display: &mut Uc8151<SPI, Output<'static>, Input<'static>, Output<'static>, Delay>,
) where
    SPI: SpiDevice,
{
    let mut name_and_details_buffer = [0; 128];
    let name_and_details = easy_format_str(
        format_args!("{}\n{}", env_value("NAME"), env_value("DETAILS")),
        &mut name_and_details_buffer,
    );

    let character_style = MonoTextStyle::new(&FONT_9X18_BOLD, BinaryColor::Off);
    let textbox_style = TextBoxStyleBuilder::new()
        .height_mode(HeightMode::FitToText)
        .alignment(HorizontalAlignment::Left)
        .paragraph_spacing(6)
        .build();

    let name_and_detail_bounds = Rectangle::new(Point::new(0, 40), Size::new(WIDTH - 75, 0));
    let name_and_detail_box = TextBox::with_textbox_style(
        &name_and_details.unwrap(),
        name_and_detail_bounds,
        character_style,
        textbox_style,
    );

    name_and_detail_box.draw(display).unwrap();

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
        .draw(display)
        .unwrap();

    Text::new(top_text.as_str(), Point::new(8, 16), character_style)
        .draw(display)
        .unwrap();

    // Draw the text box.
    // let result = display.partial_update(top_bounds.try_into().unwrap()).await;
    // match result {
    //     Ok(_) => {}
    //     Err(_) => {
    //         info!("Error updating display");
    //     }
    // }

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
        .draw(display)
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
    .draw(display)
    .unwrap();

    // let result = display
    //     .partial_update(time_bounds.try_into().unwrap())
    //     .await;
    // match result {
    //     Ok(_) => {}
    //     Err(_) => {
    //         info!("Error updating display");
    //     }
    // }

    //Manually triggered display events

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
        .draw(display)
        .unwrap();

    image.draw(display).ok();
    display.update().await.ok();
}

async fn draw_wifi<SPI>(
    display: &mut Uc8151<SPI, Output<'static>, Input<'static>, Output<'static>, Delay>,
) where
    SPI: SpiDevice,
{
    let character_style = MonoTextStyle::new(&FONT_9X18_BOLD, BinaryColor::Off);

    let top_bounds = Rectangle::new(Point::new(0, 0), Size::new(WIDTH, 24));
    top_bounds
        .into_styled(
            PrimitiveStyleBuilder::default()
                .stroke_color(BinaryColor::Off)
                .fill_color(BinaryColor::On)
                .stroke_width(1)
                .build(),
        )
        .draw(display)
        .unwrap();

    let top_text: String<64> = easy_format::<64>(format_args!(
        "Wifi found: {}",
        WIFI_COUNT.load(core::sync::atomic::Ordering::Relaxed)
    ));

    Text::new(top_text.as_str(), Point::new(8, 16), character_style)
        .draw(display)
        .unwrap();

    // let result = display.partial_update(top_bounds.try_into().unwrap()).await;
    // match result {
    //     Ok(_) => {}
    //     Err(_) => {
    //         info!("Error updating display");
    //     }
    // }

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
            .draw(display)
            .unwrap();

        Text::new(wifi.trim(), Point::new(8, y_offset + 16), character_style)
            .draw(display)
            .unwrap();

        // let result = display
        //     .partial_update(wifi_bounds.try_into().unwrap())
        //     .await;
        // match result {
        //     Ok(_) => {}
        //     Err(_) => {
        //         info!("Error updating display");
        //     }
        // }
        y_offset += 24;
    }

    display.update().await.ok();
}
