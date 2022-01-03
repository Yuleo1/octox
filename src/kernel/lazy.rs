use crate::sync::Once;
use core::cell::{Cell, UnsafeCell};
use core::marker;
use core::mem::MaybeUninit;
use core::ops::Deref;

pub struct SyncOnceCell<T> {
    once: Once,
    value: UnsafeCell<MaybeUninit<T>>,
    _marker: marker::PhantomData<T>,
}
unsafe impl<T: Sync + Send> Sync for SyncOnceCell<T> {}
unsafe impl<T: Send> Send for SyncOnceCell<T> {}

impl<T: Clone> Clone for SyncOnceCell<T> {
    fn clone(&self) -> SyncOnceCell<T> {
        let cell = Self::new();
        if let Some(value) = self.get() {
            match cell.set(value.clone()) {
                Ok(()) => (),
                Err(_) => unreachable!(),
            }
        }
        cell
    }
}

impl<T> From<T> for SyncOnceCell<T> {
    fn from(value: T) -> Self {
        let cell = Self::new();
        match cell.set(value) {
            Ok(()) => cell,
            Err(_) => unreachable!(),
        }
    }
}

impl<T: PartialEq> PartialEq for SyncOnceCell<T> {
    fn eq(&self, other: &SyncOnceCell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for SyncOnceCell<T> {}

impl<T> SyncOnceCell<T> {
    pub const fn new() -> SyncOnceCell<T> {
        SyncOnceCell {
            once: Once::new(),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            _marker: marker::PhantomData,
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
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.get_or_init(|| value.take().unwrap());
        match value {
            None => Ok(()),
            Some(value) => Err(value),
        }
    }
    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        match self.get_or_try_init(|| Ok::<T, !>(f())) {
            Ok(val) => val,
            _ => unreachable!(),
        }
    }
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.get() {
            return Ok(value);
        }
        self.initialize(f)?;

        debug_assert!(self.is_initialized());

        Ok(unsafe { self.get_unchecked() })
    }

    pub fn into_inner(mut self) -> Option<T> {
        self.take()
    }

    pub fn take(&mut self) -> Option<T> {
        if self.is_initialized() {
            self.once = Once::new();
            unsafe { Some((&mut *self.value.get()).assume_init_read()) }
        } else {
            None
        }
    }

    #[inline]
    fn is_initialized(&self) -> bool {
        self.once.is_completed()
    }

    #[cold]
    fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut res: Result<(), E> = Ok(());
        let slot = &self.value;
        self.once.call_once_force(|p| match f() {
            Ok(value) => {
                unsafe { (&mut *slot.get()).write(value) };
            }
            Err(e) => {
                res = Err(e);
                p.poison();
            }
        });
        res
    }

    unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.is_initialized());
        (&*self.value.get()).assume_init_ref()
    }

    unsafe fn get_unchecked_mut(&mut self) -> &mut T {
        debug_assert!(self.is_initialized());
        (&mut *self.value.get()).assume_init_mut()
    }
}

impl<T> Drop for SyncOnceCell<T> {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe { (&mut *self.value.get()).assume_init_drop() };
        }
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
