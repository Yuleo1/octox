use core::cell::{Cell, UnsafeCell};
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::proc::CPUS;

const BLOCKED: usize = 0;
const UNINIT: usize = 1;
const READY: usize = 2;
const POISONED: usize = 3;

pub struct SyncOnceCell<T> {
    state: AtomicUsize,
    inner: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Send for SyncOnceCell<T> where T: Send {}
unsafe impl<T> Sync for SyncOnceCell<T> where T: Send + Sync {}

struct SyncOnceCellGuard<'a, T> {
    oncecell: &'a SyncOnceCell<T>,
    poison: bool,
}

impl<T> SyncOnceCell<T> {
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(UNINIT),
            inner: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    fn try_get(&self) -> Result<&T, usize> {
        match self.state.load(Ordering::Acquire) {
            POISONED => panic!("poisoned"),
            READY => Ok(unsafe { self.get_unchecked() }),
            UNINIT => Err(UNINIT),
            BLOCKED => Err(BLOCKED),
            _ => unreachable!(),
        }
    }

    pub fn get(&self) -> Option<&T> {
        if self.is_initialized() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.is_initialized() {
            Some(unsafe { self.get_unchecked_mut() })
        } else {
            None
        }
    }

    pub fn get_or_try_init(&self, func: impl FnOnce() -> T) -> Result<&T, ()> {
        match self.try_get() {
            Ok(res) => Ok(res),
            Err(BLOCKED) => Err(()),
            Err(UNINIT) => {
                let mut func = Some(func);
                let res = self.try_init_inner(&mut || func.take().unwrap()())?;
                Ok(res)
            }
            Err(_) => unreachable!(),
        }
    }
    pub fn get_or_init(&self, func: impl FnOnce() -> T) -> &T {
        match self.get_or_try_init(func) {
            Ok(res) => res,
            Err(_) => {
                let _intr_lock = CPUS.intr_lock();
                loop {
                    if self.state.load(Ordering::Acquire) == READY {
                        break unsafe { self.get_unchecked() };
                    }
                    core::hint::spin_loop()
                }
            }
        }
    }
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.get_or_init(|| value.take().unwrap());
        match value {
            None => Ok(()),
            Some(value) => Err(value),
        }
    }

    fn try_block(&self, order: Ordering) -> Result<SyncOnceCellGuard<'_, T>, ()> {
        match self
            .state
            .compare_exchange(UNINIT, BLOCKED, order, Ordering::Relaxed)
        {
            Ok(prev) if prev == UNINIT => Ok(SyncOnceCellGuard {
                oncecell: self,
                poison: true,
            }),
            _ => Err(()),
        }
    }

    fn try_init_inner(&self, func: &mut dyn FnMut() -> T) -> Result<&T, ()> {
        unsafe {
            let mut guard = self.try_block(Ordering::Acquire)?;
            let inner = &mut *self.inner.get();
            inner.as_mut_ptr().write(func());
            guard.poison = false;
        }
        Ok(unsafe { self.get_unchecked() })
    }

    unsafe fn get_unchecked(&self) -> &T {
        (&*self.inner.get()).assume_init_ref()
    }

    unsafe fn get_unchecked_mut(&mut self) -> &mut T {
        (&mut *self.inner.get()).assume_init_mut()
    }

    fn unblock(&self, state: usize, order: Ordering) {
        self.state.swap(state, order);
    }

    pub fn into_inner(mut self) -> Option<T> {
        self.take()
    }

    fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == READY
    }

    pub fn take(&mut self) -> Option<T> {
        if self.is_initialized() {
            self.state = AtomicUsize::new(UNINIT);
            unsafe { Some((&mut *self.inner.get()).assume_init_read()) }
        } else {
            None
        }
    }
}

impl<T> Drop for SyncOnceCell<T> {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe { (&mut *self.inner.get()).assume_init_drop() }
        }
    }
}

impl<'a, T: 'a> Drop for SyncOnceCellGuard<'a, T> {
    fn drop(&mut self) {
        let state = if self.poison { POISONED } else { READY };
        self.oncecell.unblock(state, Ordering::AcqRel);
    }
}

pub struct SyncLazy<T, F = fn() -> T> {
    cell: SyncOnceCell<T>,
    init: Cell<Option<F>>,
}

unsafe impl<T, F: Send> Sync for SyncLazy<T, F> where SyncOnceCell<T>: Sync {}
// auto-derived Send impl is OK.

impl<T, F> SyncLazy<T, F> {
    pub const fn new(init: F) -> Self {
        Self {
            cell: SyncOnceCell::new(),
            init: Cell::new(Some(init)),
        }
    }
}

impl<T, F: FnOnce() -> T> SyncLazy<T, F> {
    pub fn force(this: &SyncLazy<T, F>) -> &T {
        this.cell.get_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => panic!("Lazy instance has previously been poisoned"),
        })
    }
}

impl<T, F: FnOnce() -> T> Deref for SyncLazy<T, F> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        SyncLazy::force(self)
    }
}
