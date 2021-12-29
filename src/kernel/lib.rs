#![no_std]
#![feature(asm, alloc_error_handler)]
#![feature(untagged_unions)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(new_uninit)]
#![feature(allocator_api)]
#![feature(maybe_uninit_extra)]
#![feature(negative_impls)]
#![feature(const_fn_trait_bound)]
#![feature(associated_type_bounds)]

extern crate alloc;

pub mod channel;
pub mod condvar;
pub mod console;
pub mod entry;
pub mod kernelvec;
pub mod memlayout;
pub mod param;
pub mod proc;
pub mod rwlock;
pub mod semaphore;
pub mod sleeplock;
pub mod spinlock;
pub mod start;
pub mod uart;
#[macro_use]
pub mod printf;
pub mod bio;
pub mod buddy;
pub mod file;
pub mod fs;
pub mod kalloc;
pub mod lazy;
pub mod list;
pub mod plic;
pub mod riscv;
pub mod swtch;
pub mod trampoline;
pub mod trap;
pub mod virtio_disk;
pub mod vm;
pub mod log;

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
