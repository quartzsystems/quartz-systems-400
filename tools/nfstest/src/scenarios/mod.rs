// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every scenario, in the order they run.

mod attrs;
mod dirs;
pub mod files;
mod mount;
mod names;
mod nlm;
pub mod probe;
mod rpc;
mod v40;
mod v41;

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::ctx::Ctx;
use crate::target::Vers;
use crate::{Error, Outcome, Report};

/// What a scenario returns: the lines it observed.
pub type Lines = Result<Vec<String>, Error>;

/// Which runs a scenario belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `--vers 3`: RPC, MOUNT, NFS version 3, NLM, NSM.
    V3,
    /// `--vers 4.0` and `--vers 4.1`.
    V4,
    /// `--vers 4.0`.
    V40,
    /// `--vers 4.1`.
    V41,
}

/// One scenario.
#[derive(Clone, Copy)]
pub struct Scenario {
    /// Its name.
    pub name: &'static str,
    /// Its run.
    pub family: Family,
    /// The scenario.
    pub run: fn(&Ctx) -> Lines,
}

impl std::fmt::Debug for Scenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// Every scenario.
pub fn all() -> Vec<Scenario> {
    let mut v = Vec::new();
    v.extend(rpc::LIST);
    v.extend(mount::LIST);
    v.extend(attrs::LIST);
    v.extend(files::LIST);
    v.extend(dirs::LIST);
    v.extend(names::LIST);
    v.extend(nlm::LIST);
    v.extend(v40::LIST);
    v.extend(v41::LIST);
    v
}

/// The names of every scenario.
pub fn names() -> Vec<&'static str> {
    all().iter().map(|s| s.name).collect()
}

fn in_run(f: Family, v: Vers) -> bool {
    matches!(
        (f, v),
        (Family::V3, Vers::V3)
            | (Family::V4, Vers::V40 | Vers::V41)
            | (Family::V40, Vers::V40)
            | (Family::V41, Vers::V41)
    )
}

/// Runs one scenario, a panic being a failure.
pub fn run_one(c: &Ctx, s: &Scenario) -> Outcome {
    match catch_unwind(AssertUnwindSafe(|| (s.run)(c))) {
        Ok(Ok(lines)) => Outcome::Ok(lines),
        Ok(Err(Error::Skip(why))) => Outcome::Skip(why),
        Ok(Err(e)) => Outcome::Fail(e.to_string()),
        Err(p) => {
            let why = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            Outcome::Fail(format!("panicked: {why}"))
        }
    }
}

/// Runs every selected scenario of the chosen version.
pub fn run_all(c: &Ctx) -> Report {
    let mut r = Report::default();
    for s in all() {
        let named = !c.t.only.is_empty() && c.t.wants(s.name);
        if !(named || (c.t.only.is_empty() && in_run(s.family, c.t.vers))) {
            continue;
        }
        r.push(s.name, run_one(c, &s));
    }
    r
}

/// Bytes as hex digits.
pub fn hex(b: &[u8]) -> String {
    use std::fmt::Write;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// A line `key: value`.
pub fn line(k: &str, v: impl std::fmt::Display) -> String {
    format!("{k}: {v}")
}
