// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The part of the ILE C runtime RPG programs bind to through
//! `BNDDIR(QC2LE)`: memory and string calls (`QC2UTIL2`), number and
//! hex conversion and `strtok` (`QC2UTIL1`), `sscanf` (`QC2IO`), the
//! code conversion calls (`QTQICONV`), `tmpnam` on the IFS (`QC2IFS`)
//! and the MI built-in `_CIPHER` (an MD5 digest). Strings are EBCDIC
//! and NUL-terminated as C has them; pointers arrive as fat pointers.

mod cipher;
mod conv;
mod string;

use std::cell::RefCell;
use std::collections::HashMap;

use mi::{FatPtr, Job};

use crate::net::Stable;

/// One job's runtime state: where `strtok` goes on, the names
/// `tmpnam` handed out.
#[derive(Default)]
struct State {
    strtok: Option<FatPtr>,
    tmpnames: Vec<Stable>,
    tmpcount: u32,
}

thread_local! {
    static STATES: RefCell<HashMap<usize, State>> = RefCell::new(HashMap::new());
}

fn key(j: &Job) -> usize {
    std::ptr::from_ref(j) as usize
}

fn with_state<T>(j: &Job, f: impl FnOnce(&mut State) -> T) -> T {
    STATES.with(|m| f(m.borrow_mut().entry(key(j)).or_default()))
}

/// Forget a job's state as it goes away.
pub(crate) fn drop_state(j: &Job) {
    STATES.with(|m| m.borrow_mut().remove(&key(j)));
}

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    let mut v = string::procedures();
    v.extend(conv::procedures());
    v.extend(cipher::procedures());
    v
}
