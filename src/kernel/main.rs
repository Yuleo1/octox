#![no_std]
#![no_main]

extern crate alloc;

use core::sync::atomic::{AtomicBool, Ordering};
use kernel::{
    console, kalloc, kmain, plic, print, println,
    proc::{self, Cpus},
    trap, vm, bio,
};
static STARTED: AtomicBool = AtomicBool::new(false);

kmain!(main);

extern "C" fn main() -> ! {
    let cpuid = unsafe { Cpus::cpu_id() };
    if cpuid == 0 {
        console::init();
        println!("");
        println!("octox kernel is booting");
        println!("");
        kalloc::init(); // physical memory allocator
        vm::kinit(); // create kernel page table
        vm::kinithart(); // turn on paging
        proc::init(); // process table
        trap::inithart(); // install kernel trap vector
        plic::init(); // set up interrupt controller
        plic::inithart(); // ask PLIC for device interrupts
        bio::init(); // buffer cache
        println!("yeah!!!!!");
        STARTED.store(true, Ordering::SeqCst);
    } else {
        while !STARTED.load(Ordering::SeqCst) {}
        println!("hart {} starting", unsafe { Cpus::cpu_id() });
        vm::kinithart(); // turn on paging
        trap::inithart(); // install kernel trap vector
        plic::inithart(); // ask PLIC for device interrups
    }
    loop {}
}
