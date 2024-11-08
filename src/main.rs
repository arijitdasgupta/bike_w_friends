#![no_std]
#![no_main]

mod bg;
mod game;

use core::{cell::RefCell, ops::Neg};
use critical_section::Mutex;
use defmt::info;
use defmt_rtt as _;
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use embedded_hal::{delay::DelayNs, digital::OutputPin};
use game::Game;
use hal::fugit::RateExtU32;
use heapless::{spsc::Queue, Vec};
use panic_probe as _;
use rp_pico::hal::{
    self,
    clocks::{init_clocks_and_plls, ClocksManager},
    entry,
    gpio::{self, FunctionI2C, Interrupt::EdgeLow, Pin},
    multicore::{Multicore, Stack},
    pac::{self, interrupt},
    rosc::RingOscillator,
    Timer, Watchdog,
};
use ssd1306::{mode::BufferedGraphicsMode, prelude::*, I2CDisplayInterface, Ssd1306};

use bg::Background;

pub type DisplayType = Ssd1306<
    I2CInterface<
        hal::I2C<
            pac::I2C1,
            (
                Pin<gpio::bank0::Gpio2, gpio::FunctionI2c, gpio::PullUp>,
                Pin<gpio::bank0::Gpio3, gpio::FunctionI2c, gpio::PullUp>,
            ),
        >,
    >,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;

type LeftBt = gpio::Pin<gpio::bank0::Gpio16, gpio::FunctionSioInput, gpio::PullUp>;
type CenterBt = gpio::Pin<gpio::bank0::Gpio17, gpio::FunctionSioInput, gpio::PullUp>;
type RightBt = gpio::Pin<gpio::bank0::Gpio18, gpio::FunctionSioInput, gpio::PullUp>;
type InputButtons<'a> = (LeftBt, CenterBt, RightBt);

static INPUT_IRC_SHARED: Mutex<RefCell<Option<InputButtons>>> = Mutex::new(RefCell::new(None));

// Stack for core1
static mut CORE1_STACK: Stack<8192> = Stack::new();

pub enum ButtonInput {
    Left,
    Center,
    Right,
}

// Queue to communicate between IRQ and core1
static mut INPUT_Q: Queue<ButtonInput, 100> = Queue::new();

fn core1_task(clocks: ClocksManager) -> ! {
    let mut peripherals = unsafe { pac::Peripherals::steal() };
    let sio = hal::Sio::new(peripherals.SIO);

    // Set the pins to their default state
    let pins = gpio::Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    let mut timer = hal::Timer::new(peripherals.TIMER, &mut peripherals.RESETS, &clocks);
    // Configure two pins as being I²C, not GPIO
    let sda_pin: Pin<_, FunctionI2C, _> = pins.gpio2.reconfigure();
    let scl_pin: Pin<_, FunctionI2C, _> = pins.gpio3.reconfigure();

    // Initializing display interface, display & text style etc.
    let i2c = hal::I2C::i2c1(
        peripherals.I2C1,
        sda_pin,
        scl_pin, // Try `not_an_scl_pin` here
        1.MHz(),
        &mut peripherals.RESETS,
        &clocks.system_clock,
    );
    let interface = I2CDisplayInterface::new(i2c);
    let mut display: DisplayType =
        Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();
    display.init().unwrap();

    // Status pin
    let mut led_pin = pins.gpio25.into_push_pull_output();

    // Ring oscillator
    let rosc = RingOscillator::new(peripherals.ROSC);
    let rosc = rosc.initialize_with_freq(1.kHz());

    // Button interrupts
    let bt_left = pins.gpio16.into_pull_up_input();
    let bt_center = pins.gpio17.into_pull_up_input();
    let bt_right = pins.gpio18.into_pull_up_input();
    bt_left.set_interrupt_enabled(EdgeLow, true);
    bt_center.set_interrupt_enabled(EdgeLow, true);
    bt_right.set_interrupt_enabled(EdgeLow, true);

    // Inter IR routine queue setup
    let (_, mut rx) = unsafe { INPUT_Q.split() };

    // Initializing shared pins between main and interrupts
    critical_section::with(|cs| {
        INPUT_IRC_SHARED
            .borrow(cs)
            .replace(Some((bt_left, bt_center, bt_right)));
    });

    // Game systems & states
    let mut background = Background::new();
    let mut game = Game::new();

    // Enabling interrupts
    unsafe {
        pac::NVIC::unmask(pac::Interrupt::IO_IRQ_BANK0);
    }

    info!("inited core1 & IO interrupts");
    let _ = led_pin.set_high();

    loop {
        // Updating states
        while let Some(input) = rx.dequeue() {
            match input {
                ButtonInput::Left => game.process_input(input),
                ButtonInput::Center => game.process_input(input),
                ButtonInput::Right => game.process_input(input),
            }
        }

        // Do a tick with three random bits of three friends
        let mut random_bits: Vec<bool, 3> = Vec::new();
        let _ = random_bits.push(rosc.get_random_bit());
        let _ = random_bits.push(rosc.get_random_bit());
        let _ = random_bits.push(rosc.get_random_bit());
        game.tick(random_bits);
        background.shift_bg(game.player_velocity.neg());

        // Drawing display
        let _ = display.clear(BinaryColor::Off);
        background.draw_bg(&mut display);
        game.draw_player_character(&mut display);
        game.draw_friend_characters(&mut display);
        game.draw_score(&mut display);
        display.flush().unwrap();
    }
}

// This interrupt is called from core1, handles inputs, sends to queue.
#[interrupt]
fn IO_IRQ_BANK0() {
    static mut INPUTS: Option<InputButtons> = None;

    let (mut tx, _) = unsafe { INPUT_Q.split() };

    if INPUTS.is_none() {
        critical_section::with(|cs| {
            *INPUTS = INPUT_IRC_SHARED.borrow(cs).take();
        })
    }

    if let Some(state_stuff) = INPUTS {
        let (left, center, right) = state_stuff;

        if left.interrupt_status(EdgeLow) {
            let _ = tx.enqueue(ButtonInput::Left);
            info!("left");
            left.clear_interrupt(EdgeLow);
        } else if center.interrupt_status(EdgeLow) {
            let _ = tx.enqueue(ButtonInput::Center);
            info!("center");
            center.clear_interrupt(EdgeLow);
        } else if right.interrupt_status(EdgeLow) {
            let _ = tx.enqueue(ButtonInput::Right);
            info!("right");
            right.clear_interrupt(EdgeLow);
        }
    }
}

#[entry]
fn main() -> ! {
    // Core stuff & timers
    let mut peripherals = pac::Peripherals::take().unwrap();
    let _core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(peripherals.WATCHDOG);
    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        peripherals.XOSC,
        peripherals.CLOCKS,
        peripherals.PLL_SYS,
        peripherals.PLL_USB,
        &mut peripherals.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();
    let mut timer = Timer::new(peripherals.TIMER, &mut peripherals.RESETS, &clocks);

    // The single-cycle I/O block controls our GPIO pins
    let mut sio = hal::Sio::new(peripherals.SIO);

    // Initiazling second-core
    let mut mc = Multicore::new(&mut peripherals.PSM, &mut peripherals.PPB, &mut sio.fifo);
    let cores = mc.cores();
    let core1 = &mut cores[1];
    let _test = core1.spawn(unsafe { &mut CORE1_STACK.mem }, move || core1_task(clocks));

    info!("inited core0");

    loop {
        cortex_m::asm::wfi();
    }
}
