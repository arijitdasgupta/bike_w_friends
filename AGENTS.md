# AGENTS.md

Embedded `no_std` Rust game for the **Raspberry Pi Pico 2 (RP2350)** driving a
128x64 SSD1306 I2C OLED. Three buttons steer a cyclist while three AI "friends"
react to ring-oscillator randomness; a parallax background scrolls behind them.

> This branch targets the RP2350 (Pico 2). `main` targets the original RP2040
> Pico via the `rp-pico` BSP. The two HALs have different APIs, so a port is not
> a drop-in swap — see "Porting notes" below.

## Commands

```bash
rustup target add thumbv8m.main-none-eabihf   # required once; not installed by default
cargo build                                    # builds for thumbv8m.main-none-eabihf
cargo build --release
cargo run                                      # builds + flashes + streams defmt over RTT
```

There are no tests and no lint/CI config. `cargo test` is not meaningful here:
the crate is `#![no_std]` with a `#[entry]` main and will not run on the host.

## Flashing (Picoprobe / debug probe)

- The active runner in `.cargo/config.toml` is `probe-rs run --chip RP235x`,
  which flashes over SWD and streams `defmt` logs. A Picoprobe (CMSIS-DAP) works
  out of the box; `probe-rs` auto-detects a single attached probe.
- **Requires probe-rs >= 0.31.0.** probe-rs 0.30.0 has a flawed RP2350 reset
  sequence that loses the debug connection a few seconds after boot, because
  starting core 1 (this project uses multicore) destabilises the reset
  (probe-rs issue #3563, fixed by #3699 in 0.31.0). The failure looks like:
  `Failed to read register DRW at address 0xd0c` / `Target device did not
  respond to request` / `Failed to deconfigure device during shutdown`.
  Do **not** jump to 0.32.0 either: it introduced a new RP2350 flashing
  regression (#4302), so 0.31.0 is the version to pin. Install with
  `cargo install probe-rs-tools --version 0.31.0 --locked`.
- A debugprobe/Picoprobe firmware of **v2.3.1** (official
  `debugprobe_on_pico.uf2` from the raspberrypi/debugprobe releases) is what
  this was validated against.
- Wiring for a Picoprobe: Probe GP2 -> target SWCLK, Probe GP3 -> target SWDIO,
  plus a shared GND. See `debug_probes.md`.
- Alternatives: the commented-out `picotool load -u -v -x -t elf` runner flashes
  a `.elf` over USB boot mode (hold BOOTSEL while plugging in).
- `flip-link` is **optional** and currently disabled (`linker` is commented out
  in `.cargo/config.toml`). It only adds stack-overflow protection; re-enable it
  by uncommenting `linker = "flip-link"` after `cargo install flip-link`.

## Logging

`defmt` over RTT. `DEFMT_LOG = "debug"` is set in `.cargo/config.toml`, so
`info!`/`defmt` output is emitted by default. `panic-probe` prints panics
through defmt (no text panic messages on the wire). Use `defmt::info!`, not
`println!` (there is no stdout).

## RP2350 boot requirements (easy to break)

The RP2350 Boot ROM will **not** boot the image without an image definition in
the `.start_block` section. `src/main.rs` provides it:

```rust
#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();
```

`memory.x` places `.start_block` right after `.vector_table` (so it stays in the
first 4K of flash the ROM scans) and moves `.text` to start after it. The
`binary-info` feature on `rp235x-hal` enables the optional `PICOTOOL_ENTRIES`
block read by `picotool info`. If you change `memory.x`, keep these sections and
verify with `objdump -h` that `.start_block` is present near `0x10000100`.

## Architecture / control flow

Two cores, split in `src/main.rs`:

- **core0** (`#[entry] fn main`): takes peripherals, starts clocks/PLLs, spawns
  core1, then spins in `cortex_m::asm::wfi()`. It does no game work.
- **core1** (`core1_task`): steals peripherals, brings up I2C1 + SSD1306, the
  ring oscillator, buttons, and the interrupt, then runs the entire game loop
  (drain input queue -> tick -> draw -> `display.flush()`).

Cross-core / IRQ communication uses two statics in `main.rs`:

- `INPUT_IRC_SHARED: Mutex<RefCell<Option<InputButtons>>>` — the button pins,
  handed to the IRQ on first fire via a `critical_section` and cached in a
  `static mut INPUTS` inside `IO_IRQ_BANK0`.
- `INPUT_Q: Queue<ButtonInput, 100>` — heapless SPSC queue. Both the IRQ and
  core1 call `unsafe { INPUT_Q.split() }` to get their endpoint. The IRQ only
  reports `EdgeLow` (button press) and clears the interrupt per pin.

Module layout:

- `src/game.rs` — game state and rendering. `Game` holds player + friend
  offsets/velocities, a `sub_tick_counter` used to throttle updates to every
  `TICK_COUNTER_MAX` (40) ticks, and the score. Positions are `i32` pixel
  offsets around a fixed `MIDPOINT` (60); the display is only 128 px wide, so
  keep offsets within the display or characters render off-screen.
- `src/bg.rs` — parallax background. `shift_bg` asserts the shift is negative
  (the world always scrolls left). `wrap_shift_background` only handles
  negative positions and wraps at `BACKGROUND_WIDTH` (256); each layer is drawn
  twice, one screen-width apart, to make the scroll seamless.
- `src/main.rs` — hardware init, shared statics, the interrupt, and the loop.

## Drawing / rendering

`DisplayType` (aliased in `main.rs`) is a fully-specified `Ssd1306<...>` type;
drawing helpers in `game.rs`/`bg.rs` take `&mut DisplayType`. Everything is
`embedded-graphics` with `BinaryColor` (1-bit) — no greyscale/colour. The loop
clears, draws background, player, friends, score, then `flush()`es the buffer
each iteration. Draw order matters (background before characters).

Sprites are raw `.bmp` files in `sprites/` compiled in with `include_bytes!`
and parsed by `tinybmp` at startup — editing a `.bmp` requires a rebuild; the
`.xcf` files are GIMP sources for the same images. `tinybmp` needs
1-bit/paletted BMPs for `BinaryColor`.

## Memory / build wiring

- `memory.x` defines the RP2350 flash/RAM layout (2 MiB flash default, 512 KiB
  SRAM). `build.rs` copies it into `OUT_DIR` and adds that dir to the linker
  search path, and re-runs only when `memory.x` changes. Edit `memory.x` (not
  `build.rs`) to change memory layout.
- `Cargo.toml` profiles deliberately force `codegen-units = 1`, `lto = 'fat'`,
  and `opt-level = 3` even in dev, because size/perf matter on this MCU.
- `DEFMT_LOG` lives in `.cargo/config.toml` `[env]`.
- `.cargo/config.toml` rustflags include `-C target-cpu=cortex-m33` (ARMv8-M
  Mainline). The FPU is not used, but the `eabihf` target triple is still the
  correct one for RP2350 ARM cores.

## Porting notes (RP2040 `main` <-> RP2350 this branch)

If you port changes between the two branches, these APIs differ:

- **HAL crate**: `rp-pico` (BSP, re-exports `rp2040-hal`) vs `rp235x-hal`. Both
  re-export `fugit`, `entry`, `pac`, `gpio`, `clocks`, `multicore`, `rosc`,
  `Sio`, `Watchdog`, so most code is identical once the import path changes.
- **Multicore**: rp2040 takes `&mut CORE1_STACK.mem` from a `static mut`; rp2350
  uses `static CORE1_STACK: Stack<..> = Stack::new()` (not `mut`) and
  `core1.spawn(CORE1_STACK.take().unwrap(), ...)`.
- **Timer**: rp2350 has no `Timer::new`; use `Timer::new_timer0(TIMER0, ..)` /
  `new_timer1(TIMER1, ..)`. This project does not actually use the timer.
- **I2C**: `I2C::i2c1` takes a system-clock rate (`Into<HertzU32>`) as its last
  argument, so pass `clocks.system_clock.freq()` (a `HertzU32`) — a bare `u32`
  does **not** convert. `core1_task` therefore receives the clock rate, not the
  whole `ClocksManager`.
- **Boot**: rp2350 needs the `.start_block` `IMAGE_DEF` (above) and the rp2350
  `memory.x`; rp2040 uses a `boot2` section instead.

## The `cargo-generate/` directory is vestigial

This project was generated from the `rp-pico` cargo-generate template.
`cargo-generate/` still holds the template's own `Cargo.toml`, `config.toml`,
and `remove_rprs.rhai` (a script that deletes/renames files during generation).
It is **not** part of the build — the real manifest is the root `Cargo.toml` and
the real cargo config is `.cargo/config.toml`. Don't edit the `cargo-generate/`
copies expecting them to take effect.

## Conventions

- Hardware pin assignments are fixed and repeated across `main.rs`: I2C1 on
  GP2 (SDA)/GP3 (SCL) at 1 MHz, buttons on GP16/17/18 (pull-up, active-low),
  status LED on GP25 (on-board LED on a Pico 2).
- Randomness comes from `RingOscillator::get_random_bit()` at 1 kHz; three bits
  per tick feed the three friends' velocity drift in `Game::tick`.
- Tuning constants (`VELO_MAX`, `FRIEND_OFFSET_*`, `TICK_COUNTER_MAX`, etc.) are
  module-level `const`s at the top of `game.rs`/`bg.rs` — change behaviour there.
