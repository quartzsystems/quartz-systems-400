// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job stand-in: what generated code and the runtime library share
//! while a program runs. Phase 5 replaces its creation and scheduling;
//! the fields are the contract (`MIGRATE(phase5)`).

use std::collections::VecDeque;

use crate::cond::Exception;
use crate::space::Spaces;

/// The job. The `abi` header comes first so the pointer generated code
/// holds is also a pointer to the whole record.
#[repr(C)]
#[derive(Debug, Default)]
pub struct Job {
    /// The part generated code reads: the pending flag.
    pub abi: codegen::abi::Job,
    /// The exception percolating now, if any.
    pub exception: Option<Exception>,
    /// Exceptions handled by a `monitor`, oldest first.
    pub handled: Vec<Exception>,
    /// Every exception raised, oldest first; the dump reads it.
    pub raised: Vec<Exception>,
    /// Call-stack depth: procedures entered and not yet left.
    pub depth: u32,
    /// The deepest the call stack has been.
    pub max_depth: u32,
    /// Spaces pointers address.
    pub spaces: Spaces,
    /// Lines `dsply` wrote; `MIGRATE(phase5)` to the job's display.
    pub console: Vec<String>,
    /// Replies `dsply` reads, oldest first.
    pub replies: VecDeque<String>,
}

impl Job {
    /// A new job with nothing pending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an exception is pending.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.abi.pending != 0
    }

    /// Clear the pending exception, as a handler outside generated code
    /// does.
    pub fn clear(&mut self) -> Option<Exception> {
        self.abi.pending = 0;
        self.exception.take()
    }
}
