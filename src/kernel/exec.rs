use crate::{
    elf::{self, ElfHdr, ProgHdr},
    fs::{IData, Path},
    log::LOG,
    param::MAXARG,
    proc::{Process, CPUS},
    riscv::{pgroundup, pteflags, PGSIZE},
    sleeplock::SleepLockGuard,
    vm::{Addr, UVAddr, Uvm, VirtAddr},
};
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use core::mem::size_of;

const fn flags2perm(flags: u32) -> usize {
    let mut perm = 0;
    if flags & 0x1 != 0 {
        perm = pteflags::PTE_X;
    }
    if flags & 0x2 != 0 {
        perm |= pteflags::PTE_W;
    }
    perm
}
// Load a program segment into pagetable at virtual address va.
// va must be page-aligned
// and the pages from va to va+sz must already be mapped.
// Returns Ok(()) on success, Err(()) on failure.
impl Uvm {
    pub fn loadseg(
        &mut self,
        va: UVAddr,
        ip_guard: &mut SleepLockGuard<IData>,
        offset: usize,
        sz: usize,
    ) -> Result<(), ()> {
        if !va.is_aligned() {
            panic!("loadseg(): va must be aligned.");
        }

        let mut i: usize = 0;

        while i < sz {
            match self.walkaddr(va + i) {
                Some(pa) => {
                    let n = if sz - i < PGSIZE { sz - i } else { PGSIZE };
                    if Ok(n) != ip_guard.read(From::from(pa), (offset + i) as u32, n) {
                        return Err(());
                    }
                }
                _ => {
                    panic!("loadseg: address should exsits");
                }
            }
            i += PGSIZE;
        }
        Ok(())
    }
}

pub fn exec(path: &Path, argv: [Option<String>; MAXARG]) -> Result<usize, ()> {
    let mut uvm: Option<Box<Uvm>> = None;
    let mut ustack = [0usize; MAXARG];
    let mut elf: ElfHdr = Default::default();
    let mut res;
    let mut sz = 0;
    {
        LOG.begin_op();
        let mut load = || -> Result<usize, ()> {
            let p = CPUS.my_proc().unwrap();
            let (_, ip) = path.namei().ok_or(())?;
            let mut ip_guard = ip.lock();

            // Load & Check ELF header
            if ip_guard.read(
                VirtAddr::Kernel(&mut elf as *mut _ as usize),
                0,
                size_of::<ElfHdr>(),
            ) != Ok(size_of::<ElfHdr>())
            {
                return Err(());
            }
            if elf.e_ident[elf::EI_MAG0] != elf::ELFMAG0
                || elf.e_ident[elf::EI_MAG1] != elf::ELFMAG1
                || elf.e_ident[elf::EI_MAG2] != elf::ELFMAG2
                || elf.e_ident[elf::EI_MAG3] != elf::ELFMAG3
            {
                return Err(());
            }

            uvm = Some(p.proc_uvmcreate().ok_or(())?);

            //  Load program into memory.
            let mut phdr: ProgHdr = Default::default();
            let mut off = elf.e_phoff;
            for _ in 0..elf.e_phnum {
                if ip_guard.read(
                    VirtAddr::Kernel(&mut phdr as *mut _ as usize),
                    off as u32,
                    size_of::<ProgHdr>(),
                ) != Ok(size_of::<ProgHdr>())
                {
                    return Err(());
                }
                if phdr.p_type != elf::PT_LOAD || phdr.p_fsize == 0 {
                    continue;
                }
                if phdr.p_msize < phdr.p_fsize {
                    return Err(());
                }
                if phdr.p_vaddr + phdr.p_msize < phdr.p_msize {
                    return Err(());
                }
                if phdr.p_vaddr % PGSIZE != 0 {
                    return Err(());
                }
                sz = uvm
                    .as_mut()
                    .unwrap()
                    .alloc(sz, phdr.p_vaddr + phdr.p_msize, flags2perm(phdr.p_flags))
                    .ok_or(())?;
                uvm.as_mut().unwrap().loadseg(
                    From::from(phdr.p_vaddr),
                    &mut ip_guard,
                    phdr.p_offset,
                    phdr.p_fsize,
                )?;
                off += size_of::<ProgHdr>();
            }
            Ok(0)
        };
        res = load();
        LOG.end_op();
    }

    let exec = || -> Result<usize, ()> {
        res?;
        let p = CPUS.my_proc().unwrap();
        let proc_data = unsafe { &mut *p.data.get() };
        let tf = unsafe { proc_data.trapframe.unwrap().as_mut() };
        let oldsz = unsafe { (&*p.data.get()).sz };

        // Allocate two pages at the next page boundary.
        // Make the first inaccessible as a stack guard.
        // Use the second as the user stack.
        sz = pgroundup(sz);
        sz = uvm
            .as_mut()
            .unwrap()
            .alloc(sz, sz + 2 * PGSIZE, pteflags::PTE_W)
            .ok_or(())?;
        uvm.as_mut().unwrap().clear(From::from(sz - 2 * PGSIZE));
        let mut sp = sz;
        let stackbase = sp - PGSIZE;

        // Push argument strings, prepare rest of stack in ustack.
        let mut argc = 0;
        for arg in argv
            .into_iter()
            .take_while(|e| e.is_some())
            .filter_map(|s| s)
        {
            sp -= arg.len(); // todo
            sp -= sp % 16; // riscv sp must be 16-byte aligned
            if sp < stackbase {
                return Err(());
            }
            unsafe { uvm.as_mut().unwrap().copyout(From::from(sp), &arg) }?;
            *ustack.get_mut(argc).ok_or(())? = sp;
            argc += 1;
        }
        argc += 1;
        *ustack.get_mut(argc).ok_or(())? = 0;

        // Push array of argv[] pointers.
        sp -= (argc + 1) * size_of::<usize>();
        sp -= sp % 16;
        if sp < stackbase {
            return Err(());
        }
        unsafe { uvm.as_mut().unwrap().copyout(From::from(sp), &ustack) }?;

        // arguments to user main(argc, argv)
        // argc is returned via the system call return
        // value, which goes in a0.
        unsafe { (&*p.data.get()).trapframe.unwrap().as_mut() }.a1 = sp;

        // Save program name for debugging.
        match path.file_name() {
            Some(name) => proc_data.name = name.to_string(),
            None => (),
        }

        // Commit to the user image.
        let olduvm = proc_data.uvm.replace(uvm.take().unwrap());
        proc_data.sz = sz;
        tf.epc = elf.e_entry; // initial program counter = main
        tf.sp = sp; // initial stack pointer
        olduvm.unwrap().proc_uvmfree(oldsz);

        Ok(argc) // this ends up in a0, the first argument to main(argc, argv)
    };
    res = exec();

    if res.is_err() {
        match uvm {
            Some(mut uvm) => uvm.proc_uvmfree(sz),
            _ => (),
        }
    }

    res
}
