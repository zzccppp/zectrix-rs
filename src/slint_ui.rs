extern crate alloc;

use alloc::{boxed::Box, rc::Rc};
use core::{ops::Range, time::Duration as CoreDuration};

use embassy_time::Instant;
use slint::platform::{
    Platform, PlatformError, SetPlatformError, WindowAdapter,
    software_renderer::{
        LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
    },
};

use crate::board;

slint::include_modules!();

const WIDTH: usize = board::epd::WIDTH;
const HEIGHT: usize = board::epd::HEIGHT;
const BYTES_PER_ROW_1BPP: usize = WIDTH / 8;
const BW_THRESHOLD: u16 = 200;

struct ZectrixSlintPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for ZectrixSlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> CoreDuration {
        CoreDuration::from_micros(self.start.elapsed().as_micros())
    }
}

pub fn init_platform() -> Result<Rc<MinimalSoftwareWindow>, SetPlatformError> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(slint::PhysicalSize::new(WIDTH as u32, HEIGHT as u32));

    slint::platform::set_platform(Box::new(ZectrixSlintPlatform {
        window: window.clone(),
        start: Instant::now(),
    }))?;

    Ok(window)
}

pub fn render_to_epd_frame(
    window: &MinimalSoftwareWindow,
    line_buffer: &mut [Rgb565Pixel],
    epd_frame: &mut [u8],
) -> bool {
    if epd_frame.len() < board::epd::FRAMEBUFFER_BYTES || line_buffer.len() < WIDTH {
        return false;
    }

    slint::platform::update_timers_and_animations();
    let mut rendered = false;
    window.draw_if_needed(|renderer| {
        epd_frame[..board::epd::FRAMEBUFFER_BYTES].fill(0xFF);
        renderer.render_by_line(EpdLineBuffer {
            epd_frame,
            line_buffer,
        });
        rendered = true;
    });

    rendered
}

struct EpdLineBuffer<'a> {
    epd_frame: &'a mut [u8],
    line_buffer: &'a mut [Rgb565Pixel],
}

impl LineBufferProvider for EpdLineBuffer<'_> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        if line >= HEIGHT || range.end > WIDTH || range.end > self.line_buffer.len() {
            return;
        }

        render_fn(&mut self.line_buffer[range.clone()]);

        for x in range {
            set_epd_pixel(
                self.epd_frame,
                x,
                line,
                rgb565_is_white(self.line_buffer[x]),
            );
        }
    }
}

fn rgb565_is_white(pixel: Rgb565Pixel) -> bool {
    let raw = pixel.0;
    let red = ((raw >> 11) & 0x1F) * 255 / 31;
    let green = ((raw >> 5) & 0x3F) * 255 / 63;
    let blue = (raw & 0x1F) * 255 / 31;
    let luma = (77 * red + 150 * green + 29 * blue) >> 8;

    luma >= BW_THRESHOLD
}

fn set_epd_pixel(frame: &mut [u8], x: usize, y: usize, white: bool) {
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
