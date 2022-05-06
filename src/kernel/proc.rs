use crate::file::File;
use crate::fs::{self, Inode};
use crate::lazy::{SyncLazy, SyncOnceCell};
use crate::memlayout::{kstack, TRAMPOLINE, TRAPFLAME};
use crate::spinlock::{Mutex, MutexGuard};
use crate::swtch::swtch;
use crate::trap::usertrap_ret;
use crate::vm::{Page, PageAllocator, PteFlags, UVAddr, Uvm, VirtAddr, KVM};
use crate::{param::*, riscv::*, trampoline::trampoline};
use crate::{print, println};
use alloc::string::String;
use alloc::vec::Vec;
use alloc::{boxed::Box, sync::Arc};
use array_macro::array;
use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::{cell::UnsafeCell, ops::Drop, ptr::NonNull};
use zerocopy::{AsBytes, FromBytes};

pub static CPUS: Cpus = Cpus::new();
pub static PROCS: SyncLazy<Procs> = SyncLazy::new(|| Procs::new());
pub static INITPROC: SyncOnceCell<Arc<Proc>> = SyncOnceCell::new();

// Saved registers for kernel context switches.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Context {
    pub ra: usize,
    pub sp: usize,

    // callee-saved
    pub s0: usize,
    pub s1: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
}

pub struct Cpus([UnsafeCell<Cpu>; NCPU]);
unsafe impl Sync for Cpus {}

// Per-CPU state
pub struct Cpu {
    pub proc: Option<Arc<Proc>>, // The process running on this cpu, or None.
    pub context: Context,        // swtch() here to enter scheduler().
    pub noff: UnsafeCell<isize>, // Depth of push_off()
    pub intena: bool,            // Were interrupts enabled before push_off()?
}

// Since there may be more than one IntrLock,
// make it an immutable reference to the Cpu struct.
// so, noff is wrapped in UnsafeCell.
pub struct IntrLock<'a> {
    cpu: &'a Cpu,
}

// per-process data for the trp handling code in trampoline.rs.
// sits in a page by itself just under the trampoline page in the
// user page table. not specially mapped in the kernel page table.
// the sscratch register points here.
// uservec in trampoline.rs saves user registers in the trapframe,
// then initializes registers from the trapframe's
// kernel_sp, kernel_hertid, kernel_satp, and jumps to kernel_trap.
// usertrap_ret() and userret in trampoline.rs setup
// the trapframe's kernel_*, restore user registers from the
// trapframe, switch to the user page table, and enter user space.
// the trapframe includes callee-saved user registers like s0-s11
// because the return-to-user path via usertrap_ret() doesn't return
// throgh the entire kernel call stack.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Trapframe {
    /*   0 */ pub kernel_satp: usize, // kernel page table
    /*   8 */ pub kernel_sp: usize, // top of process's kernel stack
    /*  16 */ pub kernel_trap: usize, // usertrap()
    /*  24 */ pub epc: usize, // saved user program counter
    /*  32 */ pub kernel_hartid: usize, // saved kernel tp
    /*  40 */ pub ra: usize,
    /*  48 */ pub sp: usize,
    /*  56 */ pub gp: usize,
    /*  64 */ pub tp: usize,
    /*  72 */ pub t0: usize,
    /*  80 */ pub t1: usize,
    /*  96 */ pub s0: usize,
    /* 104 */ pub s1: usize,
    /* 112 */ pub a0: usize,
    /* 120 */ pub a1: usize,
    /* 128 */ pub a2: usize,
    /* 136 */ pub a3: usize,
    /* 144 */ pub a4: usize,
    /* 152 */ pub a5: usize,
    /* 160 */ pub a6: usize,
    /* 168 */ pub a7: usize,
    /* 176 */ pub s2: usize,
    /* 184 */ pub s3: usize,
    /* 192 */ pub s4: usize,
    /* 200 */ pub s5: usize,
    /* 208 */ pub s6: usize,
    /* 216 */ pub s7: usize,
    /* 224 */ pub s8: usize,
    /* 232 */ pub s9: usize,
    /* 240 */ pub s10: usize,
    /* 248 */ pub s11: usize,
    /* 256 */ pub t3: usize,
    /* 264 */ pub t4: usize,
    /* 272 */ pub t5: usize,
    /* 280 */ pub t6: usize,
}

#[derive(Debug)]
pub struct Procs {
    pub pool: [Arc<Proc>; NPROC],
    pub wait_lock: Mutex<()>,
}
unsafe impl Sync for Procs {}

#[derive(Debug)]
pub struct Proc {
    // lock must be held when using inner data:
    pub inner: Mutex<ProcInner>,
    // lock must be held when using this:
    pub parent: UnsafeCell<Option<Arc<Proc>>>,
    // these are private to the process, so lock need not be held.
    pub data: UnsafeCell<ProcData>,
}
unsafe impl Sync for Proc {}

pub trait Process {
    fn free_proc<'a>(&self, guard: MutexGuard<'a, ProcInner>);
    fn proc_uvmcreate(&self) -> Option<Box<Uvm>>;
    fn sleep<'a, T>(&self, chan: usize, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T>;
    fn grow_proc(&self, n: isize) -> Result<(), ()>;
    fn fork(&self) -> Result<usize, ()>;
    fn exit(&self, status: i32) -> !;
    fn wait(&self, addr: UVAddr) -> Option<usize>;
    fn yielding(&self);
}

pub trait CopyInOut {
    // Copy to either a user address, or kernel address.
    // Return Result <(), ()
    fn either_copyout<T: AsBytes + ?Sized>(&self, dst: VirtAddr, src: &T) -> Result<(), ()>;
    // Copy from either a user address, or kernel address,
    // Return Result<(), ()>
    fn either_copyin<T: AsBytes + FromBytes + ?Sized>(
        &self,
        dst: &mut T,
        src: VirtAddr,
    ) -> Result<(), ()>;
}

#[derive(Clone, Copy, Debug)]
pub struct ProcInner {
    pub state: ProcState,
    pub chan: usize,
    pub killed: bool,
    pub xstate: i32,
    pub pid: PId,
}

pub struct ProcData {
    pub kstack: usize,
    pub sz: usize,
    pub uvm: Option<Box<Uvm>>,
    pub trapframe: Option<NonNull<Trapframe>>,
    pub context: Context,
    pub name: String,
    pub ofile: [Option<Arc<File>>; NOFILE],
    pub cwd: Option<Inode>,
}
unsafe impl Sync for ProcData {}
unsafe impl Send for ProcData {}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ProcState {
    UNUSED,
    USED,
    SLEEPING,
    RUNNABLE,
    RUNNING,
    ZOMBIE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PId(usize);

impl PId {
    fn alloc() -> Self {
        static NEXTID: AtomicUsize = AtomicUsize::new(0);
        PId(NEXTID.fetch_add(1, Ordering::Relaxed))
    }
}

static INITCODE: [u8; 52] = [
    0x17, 0x05, 0x00, 0x00, 0x13, 0x05, 0x45, 0x02, 0x97, 0x05, 0x00, 0x00, 0x93, 0x85, 0x35, 0x02,
    0x93, 0x08, 0x70, 0x00, 0x73, 0x00, 0x00, 0x00, 0x93, 0x08, 0x20, 0x00, 0x73, 0x00, 0x00, 0x00,
    0xef, 0xf0, 0x9f, 0xff, 0x2f, 0x69, 0x6e, 0x69, 0x74, 0x00, 0x00, 0x24, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

impl Context {
    pub const fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }
    pub fn write_zero(&mut self) {
        self.ra = 0;
        self.sp = 0;
        self.s0 = 0;
        self.s1 = 0;
        self.s2 = 0;
        self.s3 = 0;
        self.s4 = 0;
        self.s5 = 0;
        self.s6 = 0;
        self.s7 = 0;
        self.s8 = 0;
        self.s9 = 0;
        self.s10 = 0;
        self.s11 = 0;
    }
}

impl Cpus {
    const fn new() -> Self {
        Self(array![_ => UnsafeCell::new(Cpu::new()); NCPU])
    }

    // Must be called with interrupts disabled,
    // to prevent race with process being moved
    // to a different CPU.
    #[inline]
    pub unsafe fn cpu_id() -> usize {
        let id;
        asm!("mv {0}, tp", out(reg) id);
        id
    }

    // Return the pointer this Cpus's Cpu struct.
    // interrupts must be disabled.
    pub unsafe fn my_cpu(&self) -> &mut Cpu {
        let id = Self::cpu_id();
        &mut *self.0[id].get()
    }

    // intr_lock() disable interrupts on mycpu().
    // if all `intr_lock`'s are dropped, interrupts may recover
    // to previous state.
    pub fn intr_lock(&self) -> IntrLock {
        let old = intr_get();
        intr_off();
        unsafe { self.my_cpu().lock(old) }
    }

    // Return the current struct proc Some(Arc<Proc>), or None if none.
    pub fn my_proc(&self) -> Option<&Arc<Proc>> {
        let _intr_lock = self.intr_lock();
        unsafe {
            let c = self.my_cpu();
            (*c).proc.as_ref()
        }
    }

    // It is only safe to call it in Mutex's force_unlock().
    // It cannot be used anywhere else.
    pub unsafe fn intr_unlock(&self) {
        self.my_cpu().unlock();
    }
}

impl Cpu {
    const fn new() -> Self {
        Self {
            proc: None,
            context: Context::new(),
            noff: UnsafeCell::new(0),
            intena: false,
        }
    }

    // interrupts must be disabled.
    unsafe fn lock(&mut self, old: bool) -> IntrLock {
        if *self.noff.get() == 0 {
            self.intena = old;
        }
        *self.noff.get() += 1;
        IntrLock { cpu: self }
    }

    // interrupts must be disabled.
    unsafe fn unlock(&self) {
        assert!(!intr_get(), "unlock - interruptible");
        let noff = self.noff.get();
        assert!(*noff >= 1, "unlock");
        *noff -= 1;
        if *noff == 0 && self.intena {
            intr_on()
        }
    }

    // Switch to scheduler. Must hold only "proc" lock
    // and have changed proc->state . Save and restores
    // intena because intena is a property of this
    // kernel thread, not this CPU. It should
    // be proc->intena and proc->noff, but that would
    // break in the few places where a lock is held but
    // there's no processes
    unsafe fn sched<'a>(
        &mut self,
        guard: MutexGuard<'a, ProcInner>,
        ctx: &mut Context,
    ) -> MutexGuard<'a, ProcInner> {
        assert!(guard.holding(), "sched proc lock");
        assert!(*self.noff.get() == 1, "sched multiple locks");
        assert!(guard.state != ProcState::RUNNING, "shced running");
        assert!(!intr_get(), "sched interruptible");

        let intena = self.intena;
        swtch(ctx, &self.context);
        self.intena = intena;

        guard
    }
}

impl<'a> Drop for IntrLock<'a> {
    fn drop(&mut self) {
        unsafe {
            self.cpu.unlock();
        }
    }
}

impl Procs {
    pub fn new() -> Self {
        Self {
            pool: core::iter::repeat_with(|| Arc::new(Proc::new()))
                .take(NPROC)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            wait_lock: Mutex::new((), "wait lock"),
        }
    }
    // Allocate a page for each process's kernel stack.
    // map it high in memory, followed by an invalid
    // guard page.
    pub unsafe fn proc_mapstacks(&self) {
        use crate::vm::Stack;
        for (p, _) in self.pool.iter().enumerate() {
            let pa = Stack::try_new_zeroed().unwrap();
            let va = kstack(p).into();
            KVM.get_mut()
                .unwrap()
                .map(va, pa.into(), PGSIZE, PteFlags::RW);
        }
    }

    // Look in the process table for an UNUSED proc.
    // If found, initialize state required to run in the kernel,
    // and return "proc" lock held.
    // If there are no free procs, or a memory allocation fails, return None.
    pub fn alloc_proc<'a>(&'a self) -> Option<(&'a Arc<Proc>, MutexGuard<'a, ProcInner>)> {
        for p in self.pool.iter() {
            let mut lock = p.inner.lock();
            match lock.state {
                ProcState::UNUSED => {
                    lock.pid = PId::alloc();
                    lock.state = ProcState::USED;

                    let data = unsafe { &mut (*p.data.get()) };
                    // Allocate a trapframe page.
                    if let Some(tf) =
                        Page::try_new_zeroed().and_then(|t| NonNull::new(t as *mut Trapframe))
                    {
                        data.trapframe.replace(tf);
                    } else {
                        p.free_proc(lock);
                        return None;
                    }

                    // An empty user page table.
                    if let Some(uvm) = p.proc_uvmcreate() {
                        data.uvm.replace(uvm);
                    } else {
                        p.free_proc(lock);
                        return None;
                    }

                    // Set up new context to start executing at forkret,
                    // which returns to user space.
                    data.context.write_zero();
                    data.context.ra = fork_ret as usize;
                    data.context.sp = data.kstack + PGSIZE;
                    return Some((p, lock));
                }
                _ => return None,
            }
        }
        None
    }

    // Pass p's abandoned children to init.
    // Caller must hold wait_lock
    unsafe fn reparent(&self, p: &Arc<Proc>) {
        for pp in self.pool.iter() {
            if let Some(parent) = (*pp.parent.get()).as_mut() {
                if Arc::ptr_eq(parent, p) {
                    let initproc = INITPROC.get().unwrap();
                    (*pp.parent.get()).replace(Arc::clone(initproc));
                    self.wakeup(Arc::as_ptr(initproc) as usize);
                }
            }
        }
    }

    // Wake up all processes sleeping on chan.
    // Must be called without any "proc" lock.
    pub fn wakeup(&self, chan: usize) {
        for p in self.pool.iter() {
            if !Arc::ptr_eq(p, CPUS.my_proc().unwrap()) {
                let mut guard = p.inner.lock();
                if guard.state == ProcState::SLEEPING && guard.chan == chan {
                    guard.state = ProcState::RUNNABLE;
                }
            }
        }
    }

    // Kill the process with the given pid.
    // The victim won't exit until it tries to return
    // to user space (see usertrap() in trap.rs)
    pub fn kill(&self, pid: usize) -> Result<usize, ()> {
        for p in self.pool.iter() {
            let mut guard = p.inner.lock();
            if guard.pid.0 == pid {
                guard.killed = true;
                if guard.state == ProcState::SLEEPING {
                    // Wake process from sleep().
                    guard.state = ProcState::RUNNABLE;
                }
                Mutex::unlock(guard);
                return Ok(0);
            }
        }
        Err(())
    }
}

// initialize the proc table at boottime.
pub fn init() {
    for (i, proc) in PROCS.pool.iter().enumerate() {
        unsafe {
            (*proc.data.get()).kstack = kstack(i);
        }
    }
}

impl Proc {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ProcInner::new(), "proc"),
            parent: UnsafeCell::new(None),
            data: UnsafeCell::new(ProcData::new()),
        }
    }
    pub fn pid(&self) -> usize {
        self.inner.lock().pid.0
    }
}

impl Process for Arc<Proc> {
    // free a proc structure and the data hanging from it,
    // including user pages.
    // "proc" lock must be held.
    fn free_proc<'a>(&self, mut guard: MutexGuard<'a, ProcInner>) {
        let mut data = unsafe { &mut (*self.data.get()) };
        if let Some(tf) = data.trapframe.take() {
            unsafe { drop(Box::from_raw(tf.as_ptr())) }
        }
        if let Some(mut uvm) = data.uvm.take() {
            uvm.proc_uvmfree(data.sz);
        }
        data.sz = 0;
        guard.pid = PId(0);
        unsafe {
            (&mut *self.parent.get()).take();
        }
        data.name.clear();
        guard.chan = 0;
        guard.killed = false;
        guard.xstate = 0;
        guard.state = ProcState::UNUSED;
    }

    // Create a user page table for a given process,
    // with no user memory, but with trampoline pages
    fn proc_uvmcreate(&self) -> Option<Box<Uvm>> {
        // An empty page table.
        let mut uvm = Uvm::create()?;

        // map the trampoline code (for system call return)
        // at the highest user virtual address.
        // only the supervisor uses it, on the way
        // to/from user space, so not PTE_U.
        if uvm
            .mappages(
                TRAMPOLINE.into(),
                (trampoline as usize).into(),
                PGSIZE,
                PteFlags::RX,
            )
            .is_err()
        {
            uvm.free(0);
            return None;
        }

        // map the trapframe just bellow TRAMPOLINE, for trampoline.rs
        if uvm
            .mappages(
                TRAPFLAME.into(),
                (unsafe { (*self.data.get()).trapframe.unwrap().as_ptr() as usize }).into(),
                PGSIZE,
                PteFlags::RW,
            )
            .is_err()
        {
            uvm.unmap(TRAMPOLINE.into(), 1, false);
            uvm.free(0);
            return None;
        }

        Some(uvm)
    }

    // Atomically release lock and sleep on chan.
    // Reacquires lock when awakend.
    fn sleep<'a, T>(&self, chan: usize, mutex_guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        // Must acquire "proc" lock in order to
        // change p->state and then call sched.
        // Once we hold "proc" lock, we can be
        // guaranteed that we won't miss any wakeup
        // (wakeup locks "proc")
        // so it's ok to release "wait_lock".
        let mutex;
        {
            let mut lock = self.inner.lock();
            mutex = Mutex::unlock(mutex_guard);

            lock.chan = chan;
            lock.state = ProcState::SLEEPING;

            unsafe {
                lock = CPUS.my_cpu().sched(lock, &mut (*self.data.get()).context);
            }

            // Tidy up
            lock.chan = 0;
        }
        mutex.lock()
    }

    // Grow or shrink user memory by nbytes.
    // Return Result
    fn grow_proc(&self, n: isize) -> Result<(), ()> {
        let mut data = unsafe { &mut *self.data.get() };
        let mut sz = data.sz;
        let uvm = data.uvm.as_mut().unwrap();
        if n > 0 {
            sz = uvm.alloc(sz, sz + n as usize).ok_or(())?;
        } else if n < 0 {
            sz = uvm.dealloc(sz, (sz as isize + n) as usize);
        }
        data.sz = sz;
        Ok(())
    }

    // Create a new process, copying the parent.
    // Sets up child kernel stack to return as if from fork() system call.
    // call this func CPUS.myproc() => fork()
    fn fork(&self) -> Result<usize, ()> {
        let data = unsafe { &mut *self.data.get() };
        let (np, np_guard) = PROCS.alloc_proc().ok_or(())?;
        let ndata = unsafe { &mut *np.data.get() };

        // Copy user memory from parent to child.
        let uvm = data.uvm.as_mut().unwrap();
        let nuvm = ndata.uvm.as_mut().unwrap();
        if let Err(_) = uvm.copy(nuvm, data.sz) {
            np.free_proc(np_guard);
            return Err(());
        }
        ndata.sz = data.sz;

        // Copy saved user registers.
        let tf = unsafe { data.trapframe.unwrap().as_mut() };
        let ntf = unsafe { ndata.trapframe.unwrap().as_mut() };
        *ntf = *tf;

        // Cause fork to return 0 in the child
        ntf.a0 = 0;

        // increment reference counts on open file descripters.
        ndata.ofile.clone_from_slice(&data.ofile);
        // todo!() cwd idup

        ndata.name.push_str(&data.name);

        let pid = np_guard.pid;
        Mutex::unlock(np_guard);

        unsafe {
            let _wait_lock = PROCS.wait_lock.lock();
            (&mut *np.parent.get()).replace(self.clone());
        }

        np.inner.lock().state = ProcState::RUNNABLE;

        Ok(pid.0)
    }

    // Exit the current process. Does not return.
    // An Exited process remains in the zombie state
    // until its parent calls wait().
    fn exit(&self, status: i32) -> ! {
        assert!(!Arc::ptr_eq(self, INITPROC.get().unwrap()), "init exiting");

        // Close all open files
        let data = unsafe { &mut *self.data.get() };
        for fd in data.ofile.iter_mut() {
            if let Some(_file) = fd.take() {
                // file.close() todo
            }
        }

        // todo
        // begin_op();
        // iput(p->cwd);
        // end_op();
        // p->cwd = 0;

        let mut proc_guard;
        unsafe {
            let _wait_guard = PROCS.wait_lock.lock();

            // Give any children to init.
            PROCS.reparent(self);

            // Parent might be sleeping in wait().
            let pp = (*self.parent.get()).as_ref().unwrap();
            PROCS.wakeup(Arc::as_ptr(pp) as usize);

            proc_guard = self.inner.lock();
            proc_guard.xstate = status;
            proc_guard.state = ProcState::ZOMBIE;
        }

        // Jump into the scheduler
        unsafe {
            CPUS.my_cpu().sched(proc_guard, &mut data.context);
        }
        panic!("zombie exit");
    }

    // Wait for a child process to exit and return its pid.
    // Return None, if this process has no children.
    fn wait(&self, addr: UVAddr) -> Option<usize> {
        let pid;
        let mut havekids = false;
        loop {
            let wait_guard = PROCS.wait_lock.lock();
            // Scan through table looking for exited children.
            for np in PROCS.pool.iter() {
                if let Some(npp) = unsafe { &*np.parent.get() } {
                    if Arc::ptr_eq(npp, self) {
                        // make sure the child isn't still in exit() or swtch().
                        let np_guard = np.inner.lock();
                        let np_data = unsafe { &mut *np.data.get() };
                        havekids = true;
                        if np_guard.state == ProcState::ZOMBIE {
                            // Found one
                            pid = np_guard.pid.0;
                            if np_data
                                .uvm
                                .as_mut()
                                .unwrap()
                                .copyout(addr, &np_guard.xstate)
                                .is_err()
                            {
                                Mutex::unlock(np_guard);
                                Mutex::unlock(wait_guard);
                                return None;
                            }
                            np.free_proc(np_guard);
                            Mutex::unlock(wait_guard);
                            return Some(pid);
                        }
                    }
                }
            }
            // No point waiting if we don't have any children.
            if !havekids || self.inner.lock().killed {
                Mutex::unlock(wait_guard);
                break None;
            }

            // Wait for a child to exit
            self.sleep(Arc::as_ptr(self) as usize, wait_guard);
        }
    }

    fn yielding(&self) {
        let mut guard = self.inner.lock();
        guard.state = ProcState::RUNNABLE;
        unsafe {
            CPUS.my_cpu().sched(guard, &mut (*self.data.get()).context);
        }
    }
}

impl CopyInOut for Arc<Proc> {
    fn either_copyout<T: AsBytes + ?Sized>(&self, dst: VirtAddr, src: &T) -> Result<(), ()> {
        match dst {
            VirtAddr::User(addr) => {
                let uvm = unsafe { (&mut *self.data.get()).uvm.as_mut().unwrap() };
                uvm.copyout(addr.into(), src)
            }
            VirtAddr::Kernel(addr) => {
                let src = src.as_bytes();
                let len = src.len();
                let dst = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len) };
                dst.copy_from_slice(src);
                Ok(())
            }
        }
    }
    fn either_copyin<T: AsBytes + FromBytes + ?Sized>(
        &self,
        dst: &mut T,
        src: VirtAddr,
    ) -> Result<(), ()> {
        match src {
            VirtAddr::User(addr) => {
                let uvm = unsafe { (&mut *self.data.get()).uvm.as_mut().unwrap() };
                uvm.copyin(dst, addr.into())
            }
            VirtAddr::Kernel(addr) => {
                let dst = dst.as_bytes_mut();
                let len = dst.len();
                let src = unsafe { core::slice::from_raw_parts(addr as *const u8, len) };
                dst.copy_from_slice(src);
                Ok(())
            }
        }
    }
}
/*
impl CopyInOut<UVAddr> for Arc<Proc> {
    fn either_copyout<T: AsBytes>(&self, dst: UVAddr, src: &T) -> Result<(), ()> {
        let uvm = unsafe { (&mut *self.data.get()).uvm.as_mut().unwrap() };
        uvm.copyout(dst, src)
    }
    fn either_copyin<T: AsBytes + FromBytes>(&self, dst: &mut T, src: UVAddr) -> Result<(), ()> {
        let uvm = unsafe { (&mut *self.data.get()).uvm.as_mut().unwrap() };
        uvm.copyin(dst, src)
    }
}

impl CopyInOut<KVAddr> for Arc<Proc> {
    fn either_copyout<T: AsBytes>(&self, dst: KVAddr, src: &T) -> Result<(), ()> {
        let src = src.as_bytes();
        let len = src.len();
        let dst = unsafe { core::slice::from_raw_parts_mut(dst.into_usize() as *mut u8, len) };
        dst.copy_from_slice(src);
        Ok(())
    }
    fn either_copyin<T: AsBytes + FromBytes>(&self, dst: &mut T, src: KVAddr) -> Result<(), ()> {
        let dst = dst.as_bytes_mut();
        let len = dst.len();
        let src = unsafe { core::slice::from_raw_parts(src.into_usize() as *const u8, len) };
        dst.copy_from_slice(src);
        Ok(())
    }
}*/

// Set up first user process.
pub fn user_init() {
    let (p, ref mut guard) = PROCS.alloc_proc().unwrap();
    INITPROC.set(p.clone()).unwrap();
    unsafe {
        let data = &mut *p.data.get();
        // allocate one user page and copy init's instructions
        // and data into it.
        data.uvm.as_mut().unwrap().init(&INITCODE);
        data.sz = PGSIZE;

        // prepare for the very first "return" from kernel to user.
        let tf = data.trapframe.unwrap().as_mut();
        tf.epc = 0;
        tf.sp = PGSIZE;

        data.name.push_str("initcode");
        // todo!(); cwd
        guard.state = ProcState::RUNNABLE;
    }
}

impl ProcInner {
    pub const fn new() -> Self {
        Self {
            state: ProcState::UNUSED,
            chan: 0,
            killed: false,
            xstate: 0,
            pid: PId(0),
        }
    }
}

impl ProcData {
    pub fn new() -> Self {
        Self {
            kstack: 0,
            sz: 0,
            uvm: None,
            trapframe: None,
            context: Context::new(),
            name: String::new(),
            ofile: array![_ => None; NOFILE],
            cwd: Default::default(),
        }
    }
}

// A fork child's very first scheduling by shceduler()
// will swtch to fork_ret().
pub unsafe extern "C" fn fork_ret() -> ! {
    static mut FIRST: bool = true;

    // still holding "proc" lock from scheduler.
    CPUS.my_proc().unwrap().inner.force_unlock();

    if FIRST {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        FIRST = false;
        fs::init(ROOTDEV);
    }
    usertrap_ret()
}

// Print a process listing to console. For debugging.
// Runs when user types ^P on console.
// No lock to avoid wedging a stuck machine further.
pub fn procdump() {
    for proc in PROCS.pool.iter() {
        let inner = unsafe { proc.inner.get_mut() };
        let data = unsafe { &(*proc.data.get()) };
        if inner.state != ProcState::UNUSED {
            println!(
                "pid: {:?} state: {:?} name: {:?}",
                inner.pid, inner.state, data.name
            );
        }
    }
}
