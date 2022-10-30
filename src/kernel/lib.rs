#![no_std]
#![feature(alloc_error_handler)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(new_uninit)]
#![feature(allocator_api)]
#![feature(negative_impls)]
#![feature(associated_type_bounds)]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(fn_align)]
#![feature(variant_count)]
extern crate alloc;

#[cfg(target_os = "none")]
pub mod condvar;
#[cfg(target_os = "none")]
pub mod console;
#[cfg(target_os = "none")]
pub mod entry;
#[cfg(target_os = "none")]
pub mod kernelvec;
#[cfg(target_os = "none")]
pub mod memlayout;
#[cfg(target_os = "none")]
pub mod mpmc;
pub mod param;
#[cfg(target_os = "none")]
pub mod proc;
#[cfg(target_os = "none")]
pub mod rwlock;
#[cfg(target_os = "none")]
pub mod semaphore;
#[cfg(target_os = "none")]
pub mod sleeplock;
#[cfg(target_os = "none")]
pub mod spinlock;
#[cfg(target_os = "none")]
pub mod start;
#[cfg(target_os = "none")]
pub mod uart;
#[cfg(target_os = "none")]
#[macro_use]
pub mod printf;
#[cfg(target_os = "none")]
pub mod bio;
#[cfg(target_os = "none")]
pub mod buddy;
pub mod defs;
#[cfg(target_os = "none")]
pub mod elf;
#[cfg(target_os = "none")]
pub mod exec;
#[cfg(target_os = "none")]
pub mod fcntl;
pub mod file;
pub mod fs;
#[cfg(target_os = "none")]
pub mod kalloc;
#[cfg(target_os = "none")]
pub mod list;
#[cfg(target_os = "none")]
pub mod log;
#[cfg(target_os = "none")]
pub mod pipe;
#[cfg(target_os = "none")]
pub mod plic;
#[cfg(target_os = "none")]
pub mod riscv;
pub mod stat;
#[cfg(target_os = "none")]
pub mod swtch;
#[cfg(target_os = "none")]
pub mod sync;
#[cfg(target_os = "none")]
pub mod syscall;
#[cfg(target_os = "none")]
pub mod trampoline;
#[cfg(target_os = "none")]
pub mod trap;
#[cfg(target_os = "none")]
pub mod virtio_disk;
#[cfg(target_os = "none")]
pub mod vm;

#[macro_export]
macro_rules! kmain {
    ($path:path) => {
        #[export_name = "main"]
        pub extern "C" fn __main() -> ! {
            // type check the given path
            let f: extern "C" fn() -> ! = $path;

            f()
        }
    };
}
