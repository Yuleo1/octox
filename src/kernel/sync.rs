use core::cell::{Cell, UnsafeCell};
use core::marker;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct OnceLock<T> {
    once: Once,
    value: UnsafeCell<MaybeUninit<T>>,
    _marker: marker::PhantomData<T>,
}
unsafe impl<T: Sync + Send> Sync for OnceLock<T> {}
unsafe impl<T: Send> Send for OnceLock<T> {}

impl<T: Clone> Clone for OnceLock<T> {
    fn clone(&self) -> OnceLock<T> {
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

impl<T> From<T> for OnceLock<T> {
    fn from(value: T) -> Self {
        let cell = Self::new();
        match cell.set(value) {
            Ok(()) => cell,
            Err(_) => unreachable!(),
        }
    }
}

impl<T: PartialEq> PartialEq for OnceLock<T> {
    fn eq(&self, other: &OnceLock<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for OnceLock<T> {}

impl<T> OnceLock<T> {
    pub const fn new() -> OnceLock<T> {
        OnceLock {
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
        match self.get_or_try_init(|| Ok::<T, ()>(f())) {
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

impl<T> Drop for OnceLock<T> {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe { (&mut *self.value.get()).assume_init_drop() };
        }
    }
}

pub struct LazyLock<T, F = fn() -> T> {
    cell: OnceLock<T>,
    init: Cell<Option<F>>,
}

unsafe impl<T, F: Send> Sync for LazyLock<T, F> where OnceLock<T>: Sync {}
// auto-derived Send impl is OK.

impl<T, F> LazyLock<T, F> {
    pub const fn new(init: F) -> Self {
        Self {
            cell: OnceLock::new(),
            init: Cell::new(Some(init)),
        }
    }
}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    pub fn force(this: &LazyLock<T, F>) -> &T {
        this.cell.get_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => panic!("Lazy instance has previously been poisoned"),
        })
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        LazyLock::force(self)
    }
}
pub struct Once {
    state: AtomicUsize,
}

unsafe impl Sync for Once {}
unsafe impl Send for Once {}

pub struct OnceState {
    poisoned: bool,
    set_state_on_drop_to: Cell<usize>,
}

const IMCOMPLETE: usize = 0x0;
const POISONED: usize = 0x1;
const RUNNING: usize = 0x2;
const COMPLETE: usize = 0x3;

const STATE_MASK: usize = 0x3;

struct OnceGuard<'a> {
    state_and_queue: &'a AtomicUsize,
    set_state_on_drop_to: usize,
}

impl Once {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(IMCOMPLETE),
        }
    }
    pub fn call_once<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        // Fast path check
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self.call_inner(false, &mut |_| f.take().unwrap()());
    }

    pub fn call_once_force<F>(&self, f: F)
    where
        F: FnOnce(&OnceState),
    {
        // Fast path check
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self.call_inner(true, &mut |p| f.take().unwrap()(p));
    }

    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }

    #[cold]
    fn call_inner(&self, ignore_poisoning: bool, init: &mut dyn FnMut(&OnceState)) {
        let mut state_and_queue = self.state.load(Ordering::Acquire);
        loop {
            match state_and_queue {
                COMPLETE => break,
                POISONED if !ignore_poisoning => {
                    panic!("Once instance has previously poisoned");
                }
                POISONED | IMCOMPLETE => {
                    let exchange_result = self.state.compare_exchange(
                        state_and_queue,
                        RUNNING,
                        Ordering::Acquire,
                        Ordering::Acquire,
                    );
                    if let Err(old) = exchange_result {
                        state_and_queue = old;
                        continue;
                    };
                    let mut once_guard = OnceGuard {
                        state_and_queue: &self.state,
                        set_state_on_drop_to: POISONED,
                    };
                    let init_state = OnceState {
                        poisoned: state_and_queue == POISONED,
                        set_state_on_drop_to: Cell::new(COMPLETE),
                    };
                    init(&init_state);
                    once_guard.set_state_on_drop_to = init_state.set_state_on_drop_to.get();
                    break;
                }
                _ => {
                    assert!(state_and_queue & STATE_MASK == RUNNING);
                    state_and_queue = self.state.load(Ordering::Acquire);
                    core::hint::spin_loop();
                }
            }
        }
    }
}

impl Drop for OnceGuard<'_> {
    fn drop(&mut self) {
        let state_and_queue = self
            .state_and_queue
            .swap(self.set_state_on_drop_to, Ordering::AcqRel);
        assert_eq!(state_and_queue & STATE_MASK, RUNNING);
    }
}

impl OnceState {
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn poison(&self) {
        self.set_state_on_drop_to.set(POISONED);
    }
}
