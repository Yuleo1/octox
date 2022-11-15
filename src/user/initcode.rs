#![no_std]
#![feature(start)]
//#![no_main]

use ulib::usys::{exec, exit};

static init: &str = "init";

#[start]
pub fn main(argc: isize, argv: *const *const u8) -> isize {
    let a = 2;
    let v = 3;
    a - v - 1
}
