#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::timer::timg::TimerGroup;
use log::{info, warn};
use slint::ComponentHandle;
use slint::platform::software_renderer::Rgb565Pixel;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32s3 -o unstable-hal -o alloc -o wifi -o embassy -o log -o esp-backtrace

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let _power_latch = Output::new(peripherals.GPIO17, Level::High, OutputConfig::default());
    let mut led = Output::new(peripherals.GPIO3, Level::High, OutputConfig::default());
    let key_enter = Input::new(
        peripherals.GPIO0,
        InputConfig::default().with_pull(Pull::Up),
    );
    let key_down = Input::new(
        peripherals.GPIO18,
        InputConfig::default().with_pull(Pull::Up),
    );
    let key_up = Input::new(
        peripherals.GPIO39,
        InputConfig::default().with_pull(Pull::Up),
    );
    let charge_full = Input::new(peripherals.GPIO1, InputConfig::default());
    let charge_detect = Input::new(peripherals.GPIO2, InputConfig::default());

    let mut adc1_config = AdcConfig::new();
    let mut battery_pin =
        adc1_config.enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    let mut epd = zectrix_rs::epd::Epd::new(
        peripherals.GPIO6,
        peripherals.GPIO9,
        peripherals.GPIO10,
        peripherals.GPIO11,
        peripherals.GPIO8,
        peripherals.GPIO12,
        peripherals.GPIO13,
    );

    let mut timg0 = TimerGroup::new(peripherals.TIMG0);
    timg0.wdt.disable();
    let mut timg1 = TimerGroup::new(peripherals.TIMG1);
    timg1.wdt.disable();

    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("ZecTrix Note4 Rust bring-up started");
    let wifi_snapshot = zectrix_rs::wifi::scan_once(peripherals.WIFI).await;
    let slint_window = zectrix_rs::slint_ui::init_platform().unwrap();
    let ui = zectrix_rs::slint_ui::NoteUi::new().unwrap();
    ui.show().unwrap();

    let mut epd_frame = alloc::vec![0xFFu8; zectrix_rs::epd::FRAMEBUFFER_BYTES];
    let mut slint_line = alloc::vec![Rgb565Pixel(0); zectrix_rs::epd::WIDTH];
    let mut last_ui_state = UiState::INVALID;
    let mut force_ui_refresh = true;

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        let enter_raw_high = key_enter.is_high();
        let down_raw_high = key_down.is_high();
        let up_raw_high = key_up.is_high();
        let charge_detect_raw_high = charge_detect.is_high();
        let charge_full_raw_high = charge_full.is_high();
        let enter_pressed = !enter_raw_high;
        let down_pressed = !down_raw_high;
        let up_pressed = !up_raw_high;
        let charge_detect_low = !charge_detect_raw_high;
        let charge_full_high = charge_full_raw_high;
        let pin_mv = adc1.read_blocking(&mut battery_pin) as u32;
        let battery_mv = pin_mv * zectrix_rs::board::battery::VOLTAGE_DIVIDER_NUMERATOR
            / zectrix_rs::board::battery::VOLTAGE_DIVIDER_DENOMINATOR;
        let battery_percent = battery_percent_from_mv(battery_mv);
        let ui_state = UiState {
            battery_percent,
            enter_pressed,
            down_pressed,
            up_pressed,
            charge_present: charge_detect_low,
            charge_full: charge_full_high,
            wifi: wifi_snapshot,
        };

        if force_ui_refresh || ui_state != last_ui_state {
            apply_ui_state(&ui, ui_state);
            ui.window().request_redraw();

            if zectrix_rs::slint_ui::render_to_epd_frame(
                &slint_window,
                &mut slint_line,
                &mut epd_frame,
            ) {
                info!(
                    "Slint UI: refresh enter={} down={} up={} chg={} full={} bat={}% wifi={} aps={} rssi={} ch={}",
                    ui_state.enter_pressed as u8,
                    ui_state.down_pressed as u8,
                    ui_state.up_pressed as u8,
                    ui_state.charge_present as u8,
                    ui_state.charge_full as u8,
                    ui_state.battery_percent,
                    ui_state.wifi.status_text(),
                    ui_state.wifi.ap_count,
                    ui_state.wifi.best_rssi,
                    ui_state.wifi.best_channel,
                );
                if let Err(err) = epd.refresh_frame(&epd_frame).await {
                    warn!("Slint UI EPD refresh failed: {:?}", err);
                }
            } else {
                warn!("Slint UI render skipped");
            }

            last_ui_state = ui_state;
            force_ui_refresh = false;
        }

        led.set_low();
        Timer::after(Duration::from_millis(50)).await;
        led.set_high();

        info!(
            "gpio enter={} down={} up={} raw(E/D/U)={}/{}/{} chg_l={} full_h={} adc_pin={}mV vbat={}mV bat={}%",
            enter_pressed as u8,
            down_pressed as u8,
            up_pressed as u8,
            enter_raw_high as u8,
            down_raw_high as u8,
            up_raw_high as u8,
            charge_detect_low as u8,
            charge_full_high as u8,
            pin_mv,
            battery_mv,
            battery_percent,
        );
        Timer::after(Duration::from_millis(950)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

fn battery_percent_from_mv(voltage_mv: u32) -> u8 {
    if voltage_mv == 0 {
        return 0;
    }

    let mv = voltage_mv as i32;
    let percent = (-mv * mv + 9016 * mv - 19_189_000) / 10_000;
    percent.clamp(0, 100) as u8
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct UiState {
    battery_percent: u8,
    enter_pressed: bool,
    down_pressed: bool,
    up_pressed: bool,
    charge_present: bool,
    charge_full: bool,
    wifi: zectrix_rs::wifi::WifiSnapshot,
}

impl UiState {
    const INVALID: Self = Self {
        battery_percent: u8::MAX,
        enter_pressed: false,
        down_pressed: false,
        up_pressed: false,
        charge_present: false,
        charge_full: false,
        wifi: zectrix_rs::wifi::WifiSnapshot::PENDING,
    };
}

fn apply_ui_state(ui: &zectrix_rs::slint_ui::NoteUi, state: UiState) {
    ui.set_battery_percent(state.battery_percent as i32);
    ui.set_battery_text(alloc::format!("{}%", state.battery_percent).into());
    ui.set_enter_pressed(state.enter_pressed);
    ui.set_down_pressed(state.down_pressed);
    ui.set_up_pressed(state.up_pressed);
    ui.set_charge_present(state.charge_present);
    ui.set_charge_full(state.charge_full);
    ui.set_charge_text(
        if state.charge_full {
            "充满"
        } else if state.charge_present {
            "充电"
        } else {
            "电池"
        }
        .into(),
    );
    ui.set_button_text(
        if state.enter_pressed {
            "ENTER"
        } else if state.down_pressed {
            "DOWN"
        } else if state.up_pressed {
            "UP"
        } else {
            "空闲"
        }
        .into(),
    );
    ui.set_wifi_status(state.wifi.status_text().into());
    ui.set_wifi_count_text(alloc::format!("{} 个热点", state.wifi.ap_count).into());
    ui.set_wifi_rssi_text(if state.wifi.ap_count == 0 {
        "-- dBm".into()
    } else {
        alloc::format!("{} dBm", state.wifi.best_rssi).into()
    });
    ui.set_wifi_channel_text(if state.wifi.best_channel == 0 {
        "--".into()
    } else {
        alloc::format!("CH {}", state.wifi.best_channel).into()
    });
}
