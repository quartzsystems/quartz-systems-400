// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every scenario, in the order they run.

mod files;
mod missing;
mod negotiate;
mod perf;
pub mod probe;
mod scratch;
mod security;
mod signon;

use std::future::Future;
use std::time::Duration;

use crate::{Error, Outcome, Report, Target};

pub use missing::MISSING;
pub use scratch::Scratch;

/// What a scenario returns: the lines it observed.
pub type Lines = Result<Vec<String>, Error>;

/// Every scenario's name, in order, with the ones the client cannot express.
pub fn names() -> Vec<String> {
    let mut v: Vec<String> = crate::conn::DIALECTS
        .iter()
        .map(|d| format!("negotiate-{}", crate::conn::dialect_name(*d)))
        .collect();
    v.extend(
        [
            "negotiate-multiprotocol",
            "signon-good",
            "signon-wrong-password",
            "signon-unknown-user",
            "share-enum",
            "scratch-dir",
            "signing",
            "encrypt-3.0",
            "encrypt-3.0.2",
            "encrypt-3.1.1",
            "file-round-trip",
            "dir-list",
            "rename",
            "delete",
            "change-notify",
            "change-notify-two-machines",
            "large-io",
            "copy",
            "cleanup",
        ]
        .map(str::to_string),
    );
    v.extend(MISSING.iter().map(|(n, _)| (*n).to_string()));
    v
}

/// Runs one scenario under a time limit into the report.
async fn run<F: Future<Output = Lines>>(r: &mut Report, t: &Target, name: &str, limit: Duration, f: F) {
    if !t.wants(name) {
        return;
    }
    let outcome = match tokio::time::timeout(limit, f).await {
        Ok(Ok(lines)) => Outcome::Ok(lines),
        Ok(Err(Error::Inexpressible(why))) => Outcome::Skip(why),
        Ok(Err(e)) => Outcome::Fail(e.to_string()),
        Err(_) => Outcome::Fail(format!("did not finish within {}s", limit.as_secs())),
    };
    r.push(name, outcome);
}

const SHORT: Duration = Duration::from_secs(120);

/// Runs every selected scenario against `t`.
pub async fn run_all(t: &Target) -> Report {
    let mut r = Report::default();
    for d in crate::conn::DIALECTS {
        let name = format!("negotiate-{}", crate::conn::dialect_name(d));
        run(&mut r, t, &name, SHORT, Box::pin(negotiate::dialect(t, d))).await;
    }
    run(&mut r, t, "negotiate-multiprotocol", SHORT, Box::pin(negotiate::multiprotocol(t))).await;
    run(&mut r, t, "signon-good", SHORT, Box::pin(signon::good(t))).await;
    if !t.no_bad_signon {
        run(&mut r, t, "signon-wrong-password", SHORT, Box::pin(signon::wrong_password(t))).await;
        run(&mut r, t, "signon-unknown-user", SHORT, Box::pin(signon::unknown_user(t))).await;
    }
    run(&mut r, t, "share-enum", SHORT, Box::pin(signon::share_enum(t))).await;

    // Everything below writes under one run directory in the scratch directory.
    let scratch = Scratch::new(t);
    let made = if t.wants("scratch-dir") {
        run(&mut r, t, "scratch-dir", SHORT, Box::pin(scratch.create())).await;
        matches!(r.get("scratch-dir"), Some(Outcome::Ok(_)))
    } else {
        scratch.create().await.is_ok()
    };
    if made {
        run(&mut r, t, "signing", SHORT, Box::pin(security::signing(t, &scratch))).await;
        for d in [smb::Dialect::Smb030, smb::Dialect::Smb0302, smb::Dialect::Smb0311] {
            let name = format!("encrypt-{}", crate::conn::dialect_name(d));
            run(&mut r, t, &name, SHORT, Box::pin(security::encrypt(t, &scratch, d))).await;
        }
        run(&mut r, t, "file-round-trip", SHORT, Box::pin(files::round_trip(t, &scratch))).await;
        run(&mut r, t, "dir-list", SHORT, Box::pin(files::dir_list(t, &scratch))).await;
        run(&mut r, t, "rename", SHORT, Box::pin(files::rename(t, &scratch))).await;
        run(&mut r, t, "delete", SHORT, Box::pin(files::delete(t, &scratch))).await;
        run(&mut r, t, "change-notify", SHORT, Box::pin(files::change_notify(t, &scratch))).await;
        run(&mut r, t, "change-notify-two-machines", SHORT, Box::pin(files::change_notify_two_machines(t, &scratch))).await;
        run(&mut r, t, "large-io", Duration::from_secs(600), Box::pin(perf::large_io(t, &scratch))).await;
        run(&mut r, t, "copy", Duration::from_secs(3600), Box::pin(perf::copy(t, &scratch))).await;
    }
    run(&mut r, t, "cleanup", SHORT, Box::pin(scratch.remove())).await;
    for (name, why) in MISSING {
        if t.wants(name) {
            r.push(*name, Outcome::Skip(why));
        }
    }
    r
}
