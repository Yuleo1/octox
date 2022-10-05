
// Load a program segment into pagetable at virtual address va.
// va must be page-aligned
// and the pages from va to va+sz must already be mapped.
// Returns Ok(()) on success, Err(()) on failure.

use crate::{vm::{Uvm, UVAddr, Addr}, sleeplock::SleepLockGuard, fs::IData, riscv::PGSIZE};

impl Uvm {
    pub fn loadseg(&mut self, va: UVAddr, ip_guard: &mut SleepLockGuard<IData>, off: usize, sz: usize) -> Result<(), ()> {
        if !va.is_aligned() {
            panic!("loadseg(): va must be aligned.");
        }
        
        let mut i: usize = 0;

        while i < sz {

            i += PGSIZE;
        }
        Err(())
    }
}