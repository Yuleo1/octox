use core::cell::Cell;
use core::marker;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::proc::CPUS;

pub struct Once {
    state_and_queue: AtomicUsize,
    _marker: marker::PhantomData<*const Waiter>,
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

#[repr(align(4))]
struct Waiter {
    signaled: AtomicBool,
    next: *const Waiter,
}

struct WaiterQueue<'a> {
    state_and_queue: &'a AtomicUsize,
    set_state_on_drop_to: usize,
}

impl Once {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state_and_queue: AtomicUsize::new(IMCOMPLETE),
            _marker: marker::PhantomData,
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
        self.state_and_queue.load(Ordering::Acquire) == COMPLETE
    }

    #[cold]
    fn call_inner(&self, ignore_poisoning: bool, init: &mut dyn FnMut(&OnceState)) {
        let _intr_lock = CPUS.intr_lock();
        let mut state_and_queue = self.state_and_queue.load(Ordering::Acquire);
        loop {
            match state_and_queue {
                COMPLETE => break,
                POISONED if !ignore_poisoning => {
                    panic!("Once instance has previously poisoned");
                }
                POISONED | IMCOMPLETE => {
                    let exchange_result = self.state_and_queue.compare_exchange(
                        state_and_queue,
                        RUNNING,
                        Ordering::Acquire,
                        Ordering::Acquire,
                    );
                    if let Err(old) = exchange_result {
                        state_and_queue = old;
                        continue;
                    };
                    let mut waiter_queue = WaiterQueue {
                        state_and_queue: &self.state_and_queue,
                        set_state_on_drop_to: POISONED,
                    };
                    let init_state = OnceState {
                        poisoned: state_and_queue == POISONED,
                        set_state_on_drop_to: Cell::new(COMPLETE),
                    };
                    init(&init_state);
                    waiter_queue.set_state_on_drop_to = init_state.set_state_on_drop_to.get();
                    break;
                }
                _ => {
                    assert!(state_and_queue & STATE_MASK == RUNNING);
                    wait(&self.state_and_queue, state_and_queue);
                    state_and_queue = self.state_and_queue.load(Ordering::Acquire);
                }
            }
        }
    }
}

fn wait(state_and_queue: &AtomicUsize, mut current_state: usize) {
    loop {
        if current_state & STATE_MASK != RUNNING {
            return;
        }

        let node = Waiter {
            signaled: AtomicBool::new(false),
            next: (current_state & !STATE_MASK) as *const Waiter,
        };
        let me = &node as *const Waiter as usize;

        let exchange_result = state_and_queue.compare_exchange(
            current_state,
            me | RUNNING,
            Ordering::Release,
            Ordering::Relaxed,
        );
        if let Err(old) = exchange_result {
            current_state = old;
            continue;
        }
        while !node.signaled.load(Ordering::Acquire) {
            core::hint::spin_loop()
        }
        break;
    }
}

impl Drop for WaiterQueue<'_> {
    fn drop(&mut self) {
        let state_and_queue = self
            .state_and_queue
            .swap(self.set_state_on_drop_to, Ordering::AcqRel);
        assert_eq!(state_and_queue & STATE_MASK, RUNNING);
        unsafe {
            let mut queue = (state_and_queue & !STATE_MASK) as *const Waiter;
            while !queue.is_null() {
                let next = (*queue).next;
                (*queue).signaled.store(true, Ordering::Release);
                queue = next;
            }
        }
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
