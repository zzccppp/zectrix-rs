# zectrix-rs

Rust firmware workspace for the ZecTrix Note4 ESP32-S3 e-paper note hardware.

This project was generated with:

```sh
esp-generate --headless --skip-update-check --chip esp32s3 \
  --option unstable-hal --option alloc --option wifi --option embassy \
  --option log --option esp-backtrace --output-path . zectrix-rs
```

## Toolchain

- Rust toolchain: `esp`, selected by `rust-toolchain.toml`
- verified `esp` Rust version: `1.95.0-nightly`
- target: `xtensa-esp32s3-none-elf`
- linker: configured in `.cargo/config.toml`
- flasher: `espflash`

The local runner currently uses conservative flash settings for boot bring-up:

- chip: `esp32s3`
- flash size: `16mb`
- flash mode: `dio`
- flash frequency: `40mhz`

The official ESP-IDF example uses QIO/80MHz. Switch back after the Rust
bootloader/app path is confirmed stable on hardware.

## Commands

```sh
cargo build --release
cargo run --release
```

`cargo run` flashes with `espflash` and opens the serial monitor.

If the linker path changes after reinstalling the ESP Rust toolchain, refresh
`.cargo/config.toml` from:

```sh
cat ~/export-esp.sh
```

## Project Notes

- Hardware constants live in `src/board.rs`.
- The migration source of truth is `../zectrix-cpp/RUST_MIGRATION.md`.
- The template keeps `esp-radio` Wi-Fi support for factory SSID scan work.
- `embassy-net`/`smoltcp` is enabled again after upgrading the `esp` toolchain;
  the current lockfile resolves `smoltcp` to `0.13.1`.
- `slint` is added with MCU/no-std oriented features:
  `compat-1-2`, `unsafe-single-threaded`, `libm`, and `renderer-software`.
- Slint is wired through `src/slint_ui.rs`. The current UI is compiled from
  `ui/main.slint`, rendered line-by-line as RGB565, thresholded into the 1bpp
  EPD framebuffer, and then pushed through the existing EPD full-refresh path.
- The current `src/bin/main.rs` is a hardware bring-up entry: it disables timer
  watchdogs, holds `GPIO17` high for the board power latch, and blinks the
  active-low green LED on `GPIO3`. It also prints the active-low button states
  for `GPIO0/GPIO18/GPIO39`, charge status GPIOs `GPIO1/GPIO2`, and calibrated
  `GPIO4` battery ADC readings. After the `esp-rtos` scheduler is started, it
  performs one active Wi-Fi scan and displays the scan summary in Slint.
- Octal PSRAM is present on the board. Allocate the EPD framebuffer carefully
  when adding the display driver, and wire PSRAM support through `esp-hal` before
  relying on external RAM.

## Slint / E-Paper Bring-Up

`src/epd.rs` is a first-pass SSD2683-compatible EPD test driver ported from the
official C++ display code. It intentionally bit-bangs the display bus so `GPIO13`
can be switched between MOSI-style output and temperature-read input during the
controller sequence.

EPD pins:

- power enable: `GPIO6`
- busy: `GPIO8`, active low
- reset: `GPIO9`
- data/command: `GPIO10`
- chip select: `GPIO11`
- clock: `GPIO12`
- data: `GPIO13`

On boot, the current firmware:

1. starts the `esp-rtos` scheduler;
2. runs one `esp-radio` Wi-Fi scan through `src/wifi.rs`;
3. creates the Slint MCU platform and `NoteUi` component;
4. reads button, charge, and battery ADC state;
5. renders `ui/main.slint` into a 1bpp EPD framebuffer;
6. performs an EPD full refresh;
7. returns to the existing GPIO/button/battery serial debug loop.

After boot, another full refresh is requested only when the Slint-visible state
changes: button press/release, charge status, battery percentage, or future
monitoring metrics.

Expected serial output includes:

```text
INFO - WiFi scan: complete, ap_count=<n>
INFO - Slint UI: refresh enter=<0|1> down=<0|1> up=<0|1> chg=<0|1> full=<0|1> bat=<percent>% wifi=<state> aps=<n> rssi=<dBm> ch=<channel>
INFO - EPD temp raw=<value> lut=<value>
INFO - EPD refresh: complete
```

If the panel never updates, first check whether a `BusyTimeout(...)` warning was
printed. That points to power, reset, busy polarity, or command sequencing. If
the refresh runs but the image is scrambled, inspect the `pack_1bpp_to_2683`
conversion and the panel scan orientation before integrating Slint.

The Rust driver releases `GPIO6` RTC pad hold before enabling EPD power. The
official C++ firmware uses `gpio_hold_en()` on this rail, so without this step a
warm reset from the C++ firmware can leave the display power latched off.

This Slint path is intentionally full-refresh only. It proves the framework,
renderer, framebuffer conversion, and EPD driver work together. Partial refresh
and smarter dirty-region handling should be added after this baseline is stable.

## Monitoring UI Direction

`ui/main.slint` is now a first-pass Netdata-style black-and-white monitoring
screen for the 400x300 EPD. It uses compact panels for network and system state,
a small trend strip, and the existing three hardware buttons as bottom status
controls.

Chinese text is enabled by importing
`assets/fonts/SourceHanSansSC-Regular.otf`, with
`assets/fonts/SourceHanSans-LICENSE.txt` kept alongside it. This matches the
Source Han / 思源黑 family used by the official LVGL font assets more closely
than the previous Maple Mono test. Slint is configured with
`EmbedForSoftwareRenderer`, so the compiler embeds only the glyphs used by the
current UI. The source font file is still large; if the screen grows into a
larger Chinese UI, generate a Source Han subset to keep build inputs under
control.

The RGB565-to-1bpp conversion in `src/slint_ui.rs` currently uses a fixed
luminance threshold, matching the official C++ display path. Dithering looks
better for pictures and charts, but it creates holes inside small Chinese glyphs
on this 1bpp EPD. For crisp text, prefer pre-rasterized 1bpp glyphs or bitmap
font assets over anti-aliased text that is thresholded after rendering.
The official C++ assets include `SourceHanSansSC_Regular_slim` at 16px and
`SourceHanSansSC_Medium_slim` at 24px, both generated as `--bpp 1`. The Slint UI
therefore keeps Chinese text near 16px/24px and avoids very small Chinese labels.

Hardware verification:

- Slint first screen refresh is confirmed working on the ESP32-S3 board.
- Button-driven Slint property changes are confirmed to trigger EPD refreshes.
- Observed temperature readback is `raw=32`, mapping to LUT value `244`.
- Typical full-refresh timings from serial logs:
  - power-on busy wait: about `140ms`
  - display refresh busy wait: about `732ms`
  - power-off busy wait: about `135ms`
