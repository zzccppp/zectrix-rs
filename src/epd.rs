use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{
    Flex, Input, InputConfig, InputPin, Level, Output, OutputConfig, OutputPin, Pin, RtcPin,
};
use esp_hal::rom::ets_delay_us;
use log::{info, warn};

use crate::board;

pub const WIDTH: usize = board::epd::WIDTH;
pub const HEIGHT: usize = board::epd::HEIGHT;
pub const FRAMEBUFFER_BYTES: usize = board::epd::FRAMEBUFFER_BYTES;

const BYTES_PER_ROW_1BPP: usize = WIDTH / 8;
const BYTES_PER_ROW_OUT: usize = BYTES_PER_ROW_1BPP * 2;
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BusyTimeout(&'static str),
}

pub struct Epd<'d> {
    power: Output<'d>,
    reset: Output<'d>,
    dc: Output<'d>,
    cs: Output<'d>,
    busy: Input<'d>,
    sck: Output<'d>,
    sda: Flex<'d>,
}

impl<'d> Epd<'d> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        power: impl OutputPin + RtcPin + 'd,
        reset: impl OutputPin + 'd,
        dc: impl OutputPin + 'd,
        cs: impl OutputPin + 'd,
        busy: impl InputPin + 'd,
        sck: impl OutputPin + 'd,
        sda: impl Pin + 'd,
    ) -> Self {
        // The official ESP-IDF firmware uses gpio_hold_en() on the EPD power pin.
        // Release that hold first so a warm reset from the C++ firmware cannot keep
        // the display rail latched off while Rust thinks GPIO6 is high.
        power.rtcio_pad_hold(false);

        let power = Output::new(power, Level::Low, OutputConfig::default());
        let reset = Output::new(reset, Level::High, OutputConfig::default());
        let dc = Output::new(dc, Level::High, OutputConfig::default());
        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let busy = Input::new(busy, InputConfig::default());
        let sck = Output::new(sck, Level::Low, OutputConfig::default());
        let mut sda = Flex::new(sda);
        sda.apply_output_config(&OutputConfig::default());
        sda.set_low();
        sda.set_input_enable(false);
        sda.set_output_enable(true);

        Self {
            power,
            reset,
            dc,
            cs,
            busy,
            sck,
            sda,
        }
    }

    pub async fn run_test_pattern(&mut self, frame: &mut [u8]) -> Result<(), Error> {
        if frame.len() < FRAMEBUFFER_BYTES {
            warn!(
                "EPD framebuffer too small: {} < {}",
                frame.len(),
                FRAMEBUFFER_BYTES
            );
            return Ok(());
        }

        info!("EPD test: clear white");
        frame[..FRAMEBUFFER_BYTES].fill(0xFF);
        self.init().await?;
        self.display(frame).await?;
        Timer::after(Duration::from_secs(2)).await;

        info!("EPD test: full-screen diagnostic pattern");
        fill_test_pattern(&mut frame[..FRAMEBUFFER_BYTES]);
        self.init().await?;
        self.display(frame).await?;
        info!("EPD test: pattern refresh requested");

        Ok(())
    }

    pub async fn refresh_frame(&mut self, frame: &[u8]) -> Result<(), Error> {
        self.init().await?;
        self.display(frame).await
    }

    pub async fn init(&mut self) -> Result<(), Error> {
        info!("EPD init: power on, busy={}", self.busy.is_high() as u8);
        self.power_on();
        Timer::after(Duration::from_millis(10)).await;

        self.reset.set_high();
        Timer::after(Duration::from_millis(10)).await;
        self.reset.set_low();
        Timer::after(Duration::from_millis(20)).await;
        self.reset.set_high();
        Timer::after(Duration::from_millis(10)).await;
        info!(
            "EPD init: reset pulse done, busy={}",
            self.busy.is_high() as u8
        );

        self.wait_busy("init reset").await?;

        self.command(0x00);
        self.data(0x2F);
        self.data(0x2E);

        self.command(0xE9);
        self.data(0x01);
        self.wait_busy("init otp").await?;
        info!("EPD init: controller configured");
        Ok(())
    }

    pub async fn display(&mut self, frame: &[u8]) -> Result<(), Error> {
        info!("EPD display: read temperature");
        self.command(0x40);
        self.wait_busy("read temperature command").await?;
        let temp = self.read_data();
        let temp_value = match temp {
            0..=5 => 232,
            6..=10 => 235,
            11..=20 => 238,
            21..=30 => 241,
            31..=127 => 244,
            _ => 232,
        };
        info!("EPD temp raw={} lut={}", temp, temp_value);

        self.command(0xE0);
        self.data(0x02);
        self.command(0xE6);
        self.data(temp_value);

        self.command(0xA5);
        self.wait_busy("temperature load").await?;
        Timer::after(Duration::from_millis(10)).await;

        info!("EPD display: write {} bytes framebuffer", frame.len());
        self.command(0x10);
        let mut line = [0u8; BYTES_PER_ROW_OUT];
        for y in 0..HEIGHT {
            let src = &frame[y * BYTES_PER_ROW_1BPP..][..BYTES_PER_ROW_1BPP];
            let mut out = 0;
            for byte in src {
                let (a, b) = pack_1bpp_to_2683(*byte);
                line[out] = a;
                line[out + 1] = b;
                out += 2;
            }
            self.data_bytes(&line);
        }

        info!("EPD display: framebuffer transfer done");
        self.turn_on_display().await
    }

    async fn turn_on_display(&mut self) -> Result<(), Error> {
        info!("EPD refresh: power on command");
        self.command(0x04);
        self.wait_busy("power on").await?;

        info!("EPD refresh: display refresh command");
        self.command(0x12);
        self.data(0x00);
        self.wait_busy("display refresh").await?;

        info!("EPD refresh: power off command");
        self.command(0x02);
        self.data(0x00);
        self.wait_busy("power off command").await?;
        self.power_off();
        info!("EPD refresh: complete");

        Ok(())
    }

    async fn wait_busy(&self, where_: &'static str) -> Result<(), Error> {
        let start = Instant::now();
        let mut loops = 0u32;
        while self.busy.is_low() {
            if start.elapsed() >= BUSY_TIMEOUT {
                return Err(Error::BusyTimeout(where_));
            }
            loops += 1;
            Timer::after(Duration::from_millis(5)).await;
        }
        info!(
            "EPD busy: {} done, waited={}ms loops={} level={}",
            where_,
            start.elapsed().as_millis(),
            loops,
            self.busy.is_high() as u8,
        );
        Ok(())
    }

    fn command(&mut self, command: u8) {
        self.dc.set_low();
        self.cs.set_low();
        self.write_byte(command);
        self.cs.set_high();
    }

    fn data(&mut self, data: u8) {
        self.dc.set_high();
        self.cs.set_low();
        self.write_byte(data);
        self.cs.set_high();
    }

    fn data_bytes(&mut self, data: &[u8]) {
        self.dc.set_high();
        self.cs.set_low();
        for byte in data {
            self.write_byte(*byte);
        }
        self.cs.set_high();
    }

    fn read_data(&mut self) -> u8 {
        self.dc.set_high();
        self.sda.set_output_enable(false);
        self.sda.apply_input_config(&InputConfig::default());
        self.sda.set_input_enable(true);

        self.cs.set_low();
        let data = self.read_byte();
        self.cs.set_high();

        self.sda.set_input_enable(false);
        self.sda.apply_output_config(&OutputConfig::default());
        self.sda.set_low();
        self.sda.set_output_enable(true);

        data
    }

    fn write_byte(&mut self, data: u8) {
        for bit in (0..8).rev() {
            self.sck.set_low();
            if (data >> bit) & 1 == 1 {
                self.sda.set_high();
            } else {
                self.sda.set_low();
            }
            ets_delay_us(1);
            self.sck.set_high();
            ets_delay_us(1);
        }
        self.sck.set_low();
    }

    fn read_byte(&mut self) -> u8 {
        let mut data = 0u8;
        for bit in (0..8).rev() {
            self.sck.set_low();
            ets_delay_us(1);
            self.sck.set_high();
            ets_delay_us(1);
            if self.sda.is_high() {
                data |= 1 << bit;
            }
        }
        self.sck.set_low();
        data
    }

    fn power_on(&mut self) {
        self.power.set_high();
    }

    fn power_off(&mut self) {
        self.power.set_low();
    }
}

pub fn fill_test_pattern(frame: &mut [u8]) {
    frame[..FRAMEBUFFER_BYTES].fill(0xFF);

    draw_rect(frame, 0, 0, WIDTH, 4, false);
    draw_rect(frame, 0, HEIGHT - 4, WIDTH, 4, false);
    draw_rect(frame, 0, 0, 4, HEIGHT, false);
    draw_rect(frame, WIDTH - 4, 0, 4, HEIGHT, false);
    draw_rect(frame, 0, HEIGHT / 2 - 1, WIDTH, 2, false);
    draw_rect(frame, WIDTH / 2 - 1, 0, 2, HEIGHT, false);

    draw_rect(frame, 12, 12, 72, 48, false);
    draw_rect(frame, WIDTH - 84, 12, 72, 48, false);
    draw_rect(frame, 12, HEIGHT - 60, 72, 48, false);
    draw_rect(frame, WIDTH - 84, HEIGHT - 60, 72, 48, false);

    for y in 80..220 {
        for x in 110..290 {
            let black = ((x / 10) + (y / 10)) % 2 == 0;
            set_pixel(frame, x, y, !black);
        }
    }

    for i in 0..HEIGHT.min(WIDTH) {
        set_pixel(frame, i, i * HEIGHT / WIDTH, false);
        set_pixel(frame, WIDTH - 1 - i, i * HEIGHT / WIDTH, false);
    }
}

fn draw_rect(frame: &mut [u8], x: usize, y: usize, w: usize, h: usize, white: bool) {
    for yy in y..(y + h).min(HEIGHT) {
        for xx in x..(x + w).min(WIDTH) {
            set_pixel(frame, xx, yy, white);
        }
    }
}

fn set_pixel(frame: &mut [u8], x: usize, y: usize, white: bool) {
    if x >= WIDTH || y >= HEIGHT {
        return;
    }

    let index = y * BYTES_PER_ROW_1BPP + x / 8;
    let mask = 1 << (7 - (x & 7));
    if white {
        frame[index] |= mask;
    } else {
        frame[index] &= !mask;
    }
}

fn pack_1bpp_to_2683(input: u8) -> (u8, u8) {
    let mut out0 = 0u8;
    let mut out1 = 0u8;

    for i in 0..8 {
        let bit = (input >> (7 - i)) & 0x01;
        if i < 4 {
            out0 |= bit << (8 - 2 * (i + 1));
        } else {
            out1 |= bit << (14 - 2 * i);
        }
    }

    (out0, out1)
}
