// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System APIs with IBM's names and parameter layouts: programs in
//! `QSYS` a job calls by name (`QMHSNDPM`, `QUSRJOBI`, ...) and the
//! bound procedures of `QLEAWI` (`CEEDAYS`, `CEEGTST`, ...). Each API
//! reads its parameters at the addresses the caller passed, fills
//! receiver variables with the documented formats, and reports errors
//! through the error code parameter (`ERRC0100`/`ERRC0200`) or an
//! escape message when none was given (`SYSAPI.md`).
//!
//! The UIM APIs (`QUIOPNDA`, `QUIDSPP`, ...) are installed apart by
//! [`uiapi::install`]: they also need the job's display.
//!
//! Objects the APIs touch (data queues, data areas, files, system
//! values, user indexes, commands) come from a [`Provider`]: the
//! service console supplies one over the store, tests use
//! [`MemProvider`].

#![deny(missing_docs)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::match_same_arms,
    clippy::too_many_arguments,
    clippy::struct_excessive_bools
)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mi::Job;

mod cee;
mod data;
mod db;
pub mod ifs;
mod job;
pub mod mail;
mod misc;
mod msg;
pub mod net;
mod obj;
pub mod param;
pub mod picture;
mod provider;
mod regapi;
mod security;
pub mod uiapi;
mod usrspc;
mod wire;

pub use job::Civil;
pub use provider::{
    ApiFail, ApiResult, CommitInfo, DataArea, DataAreaType, DepInfo, DtaqEntry, DtaqInfo,
    FieldInfo, FileInfo, FormatInfo, IdxAttr, LocalNow, MemDtaq, MemFile, MemIndex, MemProvider,
    MemberInfo, ObjInfo, Provider, SecAnswers, SysVal, SysValKind, UserInfo,
};

/// A program API: the job, the parameter addresses, the provider.
pub type ApiFn = fn(&mut Job, &[u64], &mut dyn Provider);

/// The shared provider handle the installed closures capture.
pub type SharedProvider = Rc<RefCell<dyn Provider>>;

/// Every program API by name.
#[must_use]
pub fn programs() -> Vec<(&'static str, ApiFn)> {
    let mut v: Vec<(&'static str, ApiFn)> = Vec::new();
    v.extend(msg::APIS);
    v.extend(usrspc::APIS);
    v.extend(obj::APIS);
    v.extend(data::APIS);
    v.extend(job::APIS);
    v.extend(db::APIS);
    v.extend(misc::APIS);
    v.extend(security::APIS);
    v.extend(regapi::APIS);
    v.extend(ifs::APIS);
    v
}

/// The bound procedures of the registration facility's service
/// programs `QUSRGFA1` and `QUSRGFA2`, by export name, with their code
/// addresses.
#[must_use]
pub fn registration_procedures() -> Vec<(&'static str, usize)> {
    regapi::procedures()
}

/// Every bound procedure this crate adds (`QLEAWI`'s, then the C-style
/// socket, descriptor and resolver calls of [`net`], the IFS calls of
/// [`ifs`] and `QtmmSendMail` of [`mail`]), by export name, with its
/// code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    let mut v = cee::procedures();
    v.extend(c_procedures());
    v
}

/// The C-style procedures alone: those `ile::SYSTEM_SRVPGM_PROCS`
/// names a service program for.
#[must_use]
pub fn c_procedures() -> Vec<(&'static str, usize)> {
    let mut v = net::procedures();
    v.extend(ifs::procedures());
    v.extend(mail::procedures());
    v
}

/// Names of the `QLEAWI` exports, the runtime's own included, as the
/// binder lists them.
#[must_use]
pub fn qleawi_exports() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = mi::cee::NAMES.to_vec();
    v.extend(cee::procedures().iter().map(|(n, _)| *n));
    v
}

/// Install every API into a job: the programs as natives of `QSYS`,
/// the procedures as `*SYSTEM` imports the loader resolves. The
/// provider is shared with the caller, who may seed and inspect it.
pub fn install(job: &mut Job, provider: SharedProvider) {
    for (name, f) in programs() {
        let p = provider.clone();
        job.natives.insert(
            name.to_string(),
            Rc::new(move |j: &mut Job, args: &[u64]| {
                let mut p = p.borrow_mut();
                f(j, args, &mut *p);
            }),
        );
    }
    for (name, addr) in procedures().into_iter().chain(regapi::procedures()) {
        job.act.system.insert(name.to_string(), addr);
        // RPG upper-cases an EXTPROC name it was not given quoted.
        job.act.system.insert(name.to_ascii_uppercase(), addr);
    }
    // The provider is kept for the bound procedures that need it.
    PROVIDERS.with(|m| {
        m.borrow_mut()
            .insert(std::ptr::from_ref(job) as usize, provider);
    });
}

thread_local! {
    /// The provider of each installed job, by job address; bound
    /// procedures reach it from the environment record.
    static PROVIDERS: RefCell<HashMap<usize, SharedProvider>> = RefCell::new(HashMap::new());
}

/// The provider installed for a job, if any.
#[must_use]
pub fn provider_of(job: &Job) -> Option<SharedProvider> {
    PROVIDERS.with(|m| m.borrow().get(&(std::ptr::from_ref(job) as usize)).cloned())
}

/// Forget a job's provider when the job goes away.
pub fn uninstall(job: &Job) {
    PROVIDERS.with(|m| {
        m.borrow_mut().remove(&(std::ptr::from_ref(job) as usize));
    });
    net::drop_state(job);
    ifs::drop_state(job);
}
