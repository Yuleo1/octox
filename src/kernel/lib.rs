#![no_std]
#![feature(alloc_error_handler)]
#![feature(untagged_unions)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(new_uninit)]
#![feature(allocator_api)]
#![feature(maybe_uninit_extra)]
#![feature(negative_impls)]
#![feature(const_fn_trait_bound)]
#![feature(associated_type_bounds)]
#![feature(never_type)]

extern crate alloc;

#[cfg(target_os = "none")]
pub mod channel;
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
#[cfg(target_os = "none")]
pub mod file;
pub mod fs;
#[cfg(target_os = "none")]
pub mod kalloc;
#[cfg(target_os = "none")]
pub mod lazy;
#[cfg(target_os = "none")]
pub mod list;
#[cfg(target_os = "none")]
pub mod log;
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
