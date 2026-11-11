// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scenarios, in the order they run.

mod access;
mod files;
mod leases;
mod print;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::conn::Target;

/// Detail lines, or why it failed.
pub type Done = Result<Vec<String>, String>;
type Run = fn(Arc<Target>) -> Pin<Box<dyn Future<Output = Done> + Send>>;
type Needs = fn(&Target) -> Option<String>;

/// One scenario.
pub struct Scenario {
    pub name: &'static str,
    /// Why it cannot run with these options, if it cannot.
    pub needs: Needs,
    pub run: Run,
}

macro_rules! scenario {
    ($name:literal, $needs:expr, $f:path) => {
        Scenario {
            name: $name,
            needs: $needs,
            run: |t| Box::pin($f(t)),
        }
    };
}

fn always(_: &Target) -> Option<String> {
    None
}

fn print_share(t: &Target) -> Option<String> {
    t.opt
        .print_share
        .is_none()
        .then(|| "no --print-share".into())
}

fn driver_file(t: &Target) -> Option<String> {
    print_share(t).or_else(|| {
        t.opt
            .driver_file
            .is_none()
            .then(|| "no --driver-file".into())
    })
}

fn ifs_text(t: &Target) -> Option<String> {
    t.opt.ifs_text.is_none().then(|| "no --ifs-text".into())
}

fn guest_user(t: &Target) -> Option<String> {
    t.opt.guest_user.is_none().then(|| "no --guest-user".into())
}

fn max_share(t: &Target) -> Option<String> {
    t.opt.max_share.is_none().then(|| "no --max-share".into())
}

fn enc_share(t: &Target) -> Option<String> {
    t.opt.enc_share.is_none().then(|| "no --enc-share".into())
}

fn min_dialect(t: &Target) -> Option<String> {
    t.opt
        .min_dialect
        .is_none()
        .then(|| "no --min-dialect".into())
}

/// Every scenario.
pub static ALL: &[Scenario] = &[
    scenario!("share-enum", always, files::share_enum),
    scenario!("lock-conflict", always, files::lock_conflict),
    scenario!("lease-break", always, leases::lease_break),
    scenario!("lease-break-ifs", ifs_text, leases::lease_break_ifs),
    scenario!("durable-reconnect", always, leases::durable_reconnect),
    scenario!("print-write", print_share, print::print_write),
    scenario!("spoolss", print_share, print::spoolss),
    scenario!("print-dollar", driver_file, print::print_dollar),
    scenario!("guest-logon", guest_user, access::guest_logon),
    scenario!("max-users", max_share, access::max_users),
    scenario!("encrypt-share", enc_share, access::encrypt_share),
    scenario!("min-dialect", min_dialect, access::min_dialect),
    scenario!("cleanup", always, files::cleanup),
];
