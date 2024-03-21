#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_halt as _;

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;

const WIDTH: usize = 5;
const HEIGHT: usize = 5;

type Grid = [[bool; WIDTH]; HEIGHT];

fn count_neighbors(grid: &Grid, x: usize, y: usize) -> usize {
    let mut count = 0;
    for dx in 0..3 {
        for dy in 0..3 {
            if dx == 1 && dy == 1 {
                continue;
            }
            let nx = (x + dx + WIDTH - 1) % WIDTH;
            let ny = (y + dy + HEIGHT - 1) % HEIGHT;
            if grid[ny][nx] {
                count += 1;
            }
        }
    }
    count
}

fn update_grid(grid: &mut Grid) {
    let mut new_grid = [[false; WIDTH]; HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let neighbors = count_neighbors(grid, x, y);
            if grid[y][x] && (neighbors == 2 || neighbors == 3) {
                new_grid[y][x] = true;
            } else if !grid[y][x] && neighbors == 3 {
                new_grid[y][x] = true;
            }
        }
    }
    *grid = new_grid;
}

use microbit::{
    board::Board,
    display::nonblocking::{Display, GreyscaleImage},
    hal::{
        clocks::Clocks,
        rtc::{Rtc, RtcInterrupt},
    },
    pac::{self, interrupt, RTC0, TIMER1},
};

fn draw_grid(grid: &Grid) -> GreyscaleImage {
    let mut a = [[0u8; 5]; 5];

    for i in 0..5 {
        for j in 0..5 {
            a[i][j] = if grid[i][j] { 7 } else { 0 };
        }
    }

    GreyscaleImage::new(&a)
}

// We use TIMER1 to drive the display, and RTC0 to update the animation.
// We set the TIMER1 interrupt to a higher priority than RTC0.

static DISPLAY: Mutex<RefCell<Option<Display<TIMER1>>>> = Mutex::new(RefCell::new(None));
static ANIM_TIMER: Mutex<RefCell<Option<Rtc<RTC0>>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    if let Some(mut board) = Board::take() {
        // Starting the low-frequency clock (needed for RTC to work)
        Clocks::new(board.CLOCK).start_lfclk();

        // RTC at 16Hz (32_768 / (4095 + 1))
        // 125ms period
        let mut rtc0 = Rtc::new(board.RTC0, 4095).unwrap();
        rtc0.enable_event(RtcInterrupt::Tick);
        rtc0.enable_interrupt(RtcInterrupt::Tick, None);
        rtc0.enable_counter();

        // Create display
        let display = Display::new(board.TIMER1, board.display_pins);

        cortex_m::interrupt::free(move |cs| {
            *DISPLAY.borrow(cs).borrow_mut() = Some(display);
            *ANIM_TIMER.borrow(cs).borrow_mut() = Some(rtc0);
        });
        unsafe {
            board.NVIC.set_priority(pac::Interrupt::RTC0, 254);
            board.NVIC.set_priority(pac::Interrupt::TIMER1, 128);
            pac::NVIC::unmask(pac::Interrupt::RTC0);
            pac::NVIC::unmask(pac::Interrupt::TIMER1);
        }
    }

    loop {
        continue;
    }
}

#[interrupt]
fn TIMER1() {
    cortex_m::interrupt::free(|cs| {
        if let Some(display) = DISPLAY.borrow(cs).borrow_mut().as_mut() {
            display.handle_display_event();
        }
    });
}

#[interrupt]
unsafe fn RTC0() {
    static mut STEP: u8 = 0;
    // static mut GRID: [[bool; WIDTH]; HEIGHT] = [[false; WIDTH]; HEIGHT];
    static mut GRID: Grid = [
        [false, false, false, false, false],
        [false, true, true, true, false],
        [false, true, false, true, false],
        [false, true, true, true, false],
        [false, false, false, false, false],
    ];

    cortex_m::interrupt::free(|cs| {
        if let Some(rtc) = ANIM_TIMER.borrow(cs).borrow_mut().as_mut() {
            rtc.reset_event(RtcInterrupt::Tick);
        }
    });

    let mut grid = GRID;

    *STEP += 1;
    if *STEP > 4 {
        update_grid(&mut grid);
        *STEP = 0;
    }

    cortex_m::interrupt::free(|cs| {
        if let Some(display) = DISPLAY.borrow(cs).borrow_mut().as_mut() {
            display.show(&draw_grid(grid));
        }
    });
}
