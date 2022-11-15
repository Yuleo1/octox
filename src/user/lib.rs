#![no_std]

use core::panic;

pub mod stat;
pub mod usys;

#[panic_handler]
fn panic(_info: &panic::PanicInfo<'_>) -> ! {
    loop {}
}
