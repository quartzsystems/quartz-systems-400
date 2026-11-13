// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS, in the Network group: `/nfs` (the exports: Add, Properties,
//! Remove, Export all), `/nfs/servers` (the daemons: Start, End) and
//! `/nfs/mounts` (the mount table: Mount, Unmount). Every change is the
//! command itself (`EXPORTFS`, `STRNFSSVR`, `ENDNFSSVR`, `MOUNT`,
//! `UNMOUNT`) run as the user ([`crate::env::nfs::Nfs::run`]) after the
//! `*IOSYSCFG` check the command makes.

mod exports;
mod mounts;
mod servers;

pub use servers::TYPES;

use qsweb::http::Response;
use qsweb::partials::tabs;

use super::Rq;
use super::common::{answered, quote};
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Network";

/// Route `segs` (the path's segments, `nfs` first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["nfs"] => exports::list(rq),
        ["nfs", "confirm"] => exports::export_all(rq),
        ["nfs", "add"] => exports::edit(rq, None),
        ["nfs", "exp", id, rest @ ..] => exports::one(rq, id, rest),
        ["nfs", "servers", rest @ ..] => servers::route(rq, rest),
        ["nfs", "mounts", rest @ ..] => mounts::route(rq, rest),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A page of the Network group, below NFS.
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    let mut c = vec!["NFS"];
    c.extend_from_slice(crumb);
    super::common::group_page(rq, GROUP, &c, h1, None, body, help, item)
}

/// The NFS tabs, `current` bold.
fn nfs_tabs(rq: &Rq<'_>, current: usize) -> String {
    tabs(
        &[
            ("Exports", rq.ctx.url("/nfs")),
            ("Servers", rq.ctx.url("/nfs/servers")),
            ("Mounts", rq.ctx.url("/nfs/mounts")),
        ],
        current,
    )
}

/// Check `*IOSYSCFG`, which `cmd` always needs.
fn need(rq: &Rq<'_>, function: &str, cmd: &str, crumb: &str) -> Result<(), Response> {
    let mut t = Target::obj("QSYS", cmd, "*CMD");
    t.shown = cmd.to_string();
    rq.require(Need::IoSysCfg, function, &t, &[GROUP, "NFS", crumb])
}

/// Run `command` as the user and go `back` with its answer waiting.
fn run(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    answered(rq, rq.env().nfs.run(rq.user(), command), back)
}

/// A CL value: special values and numbers as they are, the rest quoted.
fn arg(v: &str) -> String {
    let v = v.trim();
    if v.starts_with('*') || (!v.is_empty() && v.chars().all(|c| c.is_ascii_digit())) {
        v.to_string()
    } else {
        quote(v)
    }
}

/// `*YES` or `*NO`.
fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// A time as the user's job shows it; blank for none.
fn when(rq: &Rq<'_>, secs: u64) -> String {
    if secs == 0 {
        String::new()
    } else {
        rq.when(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_cl() {
        assert_eq!(arg("*BINARY"), "*BINARY");
        assert_eq!(arg("819"), "819");
        assert_eq!(arg("@10.0.0.0/8"), "'@10.0.0.0/8'");
        assert_eq!(arg("it's"), "'it''s'");
    }
}
