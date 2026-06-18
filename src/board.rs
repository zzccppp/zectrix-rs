//! Board constants for ZecTrix Note4.
//!
//! These values are mirrored from `zectrix-cpp/RUST_MIGRATION.md` and the
//! official C++ board definition. Keep this module hardware-only; drivers
//! should live in separate modules.

pub mod pins {
    pub const KEY_ENTER: u8 = 0;
    pub const CHARGE_FULL_STDBY_H: u8 = 1;
    pub const CHARGE_DETECT_CHRG_L: u8 = 2;
    pub const LED_GREEN: u8 = 3;
    pub const BATTERY_ADC: u8 = 4;
    pub const RTC_INT: u8 = 5;
    pub const EPD_POWER_ENABLE: u8 = 6;
    pub const NFC_FD: u8 = 7;
    pub const EPD_BUSY: u8 = 8;
    pub const EPD_RESET: u8 = 9;
    pub const EPD_DC: u8 = 10;
    pub const EPD_CS: u8 = 11;
    pub const EPD_SCK: u8 = 12;
    pub const EPD_SDA: u8 = 13;
    pub const I2S_MCLK: u8 = 14;
    pub const I2S_BCLK: u8 = 15;
    pub const I2S_DIN: u8 = 16;
    pub const POWER_LATCH: u8 = 17;
    pub const KEY_DOWN_POWER_DET: u8 = 18;
    pub const USB_DN: u8 = 19;
    pub const USB_DP: u8 = 20;
    pub const NFC_POWER: u8 = 21;
    pub const I2S_WS: u8 = 38;
    pub const KEY_UP: u8 = 39;
    pub const J3_GPIO40: u8 = 40;
    pub const J3_GPIO41: u8 = 41;
    pub const AUDIO_POWER: u8 = 42;
    pub const I2S_DOUT: u8 = 45;
    pub const AUDIO_PA_ENABLE: u8 = 46;
    pub const I2C_SDA: u8 = 47;
    pub const I2C_SCL: u8 = 48;
}

pub mod i2c {
    pub const BUS_FREQUENCY_HZ: u32 = 400_000;

    pub const ES8311_ADDR: u8 = 0x18;
    pub const PCF8563_ADDR: u8 = 0x51;
    pub const GT23SC6699_ADDR: u8 = 0x55;
}

pub mod epd {
    pub const WIDTH: usize = 400;
    pub const HEIGHT: usize = 300;
    pub const FRAMEBUFFER_BYTES: usize = WIDTH * HEIGHT / 8;

    pub const WRITE_SPI_FREQUENCY_HZ: u32 = 40_000_000;
    pub const READ_SPI_FREQUENCY_HZ: u32 = 8_000_000;

    pub const BUSY_ACTIVE_LOW: bool = true;
}

pub mod audio {
    pub const SAMPLE_RATE_HZ: u32 = 16_000;
    pub const SAMPLE_BITS: u8 = 16;
    pub const DEFAULT_VOLUME: u8 = 70;
    pub const FACTORY_TEST_VOLUME: u8 = 80;
    pub const ES8311_INPUT_GAIN: u8 = 30;
}

pub mod battery {
    pub const ADC_MAX: u32 = 4095;
    pub const ADC_ATTENUATION_DB: u8 = 12;

    pub const VOLTAGE_DIVIDER_NUMERATOR: u32 = 2;
    pub const VOLTAGE_DIVIDER_DENOMINATOR: u32 = 1;
}
