use crate::{vm::{Uvm, UVAddr, Addr}, sleeplock::SleepLockGuard, fs::{IData, Path}, riscv::PGSIZE};
// Load a program segment into pagetable at virtual address va.
// va must be page-aligned
// and the pages from va to va+sz must already be mapped.
// Returns Ok(()) on success, Err(()) on failure.
impl Uvm {
    pub fn loadseg(&mut self, va: UVAddr, ip_guard: &mut SleepLockGuard<IData>, offset: usize, sz: usize) -> Result<(), ()> {
        if !va.is_aligned() {
            panic!("loadseg(): va must be aligned.");
        }
        
        let mut i: usize = 0;

        while i < sz {
            match self.walkaddr(va + i) {
                Some(pa) => {
                    let n = if sz - i < PGSIZE {
                        sz - i
                    } else {
                        PGSIZE
                    };
                    if Ok(n) != ip_guard.read(From::from(pa), (offset + i) as u32, n) {
                        return Err(());
                    }
                },
                _ => {
                    panic!("loadseg: address should exsits");
                }
            }
            i += PGSIZE;
        }
        Ok(())
    }
}

//pub fn exec(path: &Path)