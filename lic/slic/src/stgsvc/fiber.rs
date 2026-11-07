// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fibers: a long statement runs on a stack of its own, so the serve
//! loop resumes statements in any order, not last in, first out. One
//! runs at a time, on the service's one thread; a fiber gives control
//! back with [`suspend`] from as deep as the engine calls it. Reaching
//! the running fiber's yielder from there is the only `unsafe` here.

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ptr;

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};

/// A fiber's stack: as much as the service's own thread has.
const STACK: usize = 8 << 20;

/// Stacks kept for the next fibers.
const SPARE: usize = 4;

thread_local! {
    /// The yielder of the fiber running now; null in the serve loop.
    static CURRENT: Cell<*const Yielder<(), ()>> = const { Cell::new(ptr::null()) };
    static STACKS: RefCell<Vec<DefaultStack>> = const { RefCell::new(Vec::new()) };
}

/// One body on a stack of its own.
pub struct Fiber {
    co: Coroutine<(), (), (), DefaultStack>,
}

/// A stack for the next fiber: a spare one, or a new mapping.
pub fn stack() -> std::io::Result<DefaultStack> {
    match STACKS.with(|s| s.borrow_mut().pop()) {
        Some(s) => Ok(s),
        None => DefaultStack::new(STACK),
    }
}

impl Fiber {
    /// A fiber that runs `body` on `stack` when first resumed.
    pub fn on(stack: DefaultStack, body: impl FnOnce() + 'static) -> Self {
        // Boxed: what the coroutine starts with goes on its stack, at
        // most 1 KiB; a statement's session is more.
        let body: Box<dyn FnOnce()> = Box::new(body);
        let co = Coroutine::with_stack(stack, move |y: &Yielder<(), ()>, ()| {
            CURRENT.with(|c| c.set(ptr::from_ref(y)));
            body();
            CURRENT.with(|c| c.set(ptr::null()));
        });
        Self { co }
    }

    /// Run until the body suspends (false) or ends (true).
    pub fn resume(&mut self) -> bool {
        let r = self.co.resume(());
        // Back in the loop, whatever happened on the fiber.
        CURRENT.with(|c| c.set(ptr::null()));
        matches!(r, CoroutineResult::Return(()))
    }

    /// An ended fiber's stack, kept for the next.
    pub fn recycle(self) {
        if !self.co.done() {
            return;
        }
        let stack = self.co.into_stack();
        STACKS.with(|s| {
            let mut s = s.borrow_mut();
            if s.len() < SPARE {
                s.push(stack);
            }
        });
    }
}

/// Give control back to the loop that resumed the running fiber, until
/// it resumes it again. False, at once, outside a fiber.
pub fn suspend() -> bool {
    let me = CURRENT.with(Cell::get);
    if me.is_null() {
        return false;
    }
    CURRENT.with(|c| c.set(ptr::null()));
    // SAFETY: `me` is non-null only while a fiber's body runs: it is set
    // from the `&Yielder` its body was given at the body's start and
    // again below, and cleared before every suspend, when the body ends
    // and after every resume returns to the loop. Only one context runs
    // at a time on this thread, so the code here runs on that body's
    // stack, inside the call that owns the yielder: the reference is
    // live, and suspending through it is what the body itself may do.
    unsafe { (*me).suspend(()) };
    CURRENT.with(|c| c.set(me));
    true
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn fibers_take_turns_in_any_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let fiber = |name: &'static str, steps: u32| {
            let log = Rc::clone(&log);
            Fiber::on(stack().unwrap(), move || {
                for i in 0..steps {
                    log.borrow_mut().push(format!("{name}{i}"));
                    assert!(suspend());
                }
                log.borrow_mut().push(format!("{name} end"));
            })
        };
        let mut a = fiber("a", 1);
        let mut b = fiber("b", 3);
        assert!(!a.resume());
        assert!(!b.resume());
        // The first one ends while the later one is still in progress.
        assert!(a.resume());
        assert!(!b.resume());
        assert!(!suspend(), "the loop is no fiber");
        assert!(!b.resume());
        assert!(b.resume());
        a.recycle();
        b.recycle();
        assert_eq!(*log.borrow(), ["a0", "b0", "a end", "b1", "b2", "b end"]);
        // A recycled stack runs the next.
        let mut c = fiber("c", 0);
        assert!(c.resume());
    }

    #[test]
    fn a_body_holding_much_runs() {
        let big = [7u8; 4096];
        let sum = Rc::new(Cell::new(0u32));
        let out = Rc::clone(&sum);
        let mut f = Fiber::on(stack().unwrap(), move || {
            out.set(big.iter().map(|&b| u32::from(b)).sum())
        });
        assert!(f.resume());
        assert_eq!(sum.get(), 7 * 4096);
    }
}
