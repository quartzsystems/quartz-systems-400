// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! MOUNT version 3: `MNT`, `DUMP`, `UMNT`, `EXPORT`, refusals.

use nfs_mount::wire::mountstat3;

use super::{Family, Lines, Scenario, line};
use crate::ctx::Ctx;
use crate::error::{check, fail};
use crate::v3::{Mount, mnt_split};

/// This file's scenarios.
pub const LIST: [Scenario; 2] = [
    Scenario {
        name: "mount-cycle",
        family: Family::V3,
        run: cycle,
    },
    Scenario {
        name: "mount-not-exported",
        family: Family::V3,
        run: not_exported,
    },
];

fn cycle(c: &Ctx) -> Lines {
    let mut m = Mount {
        c: c.client(c.mount_addr()?),
    };
    let path = c.t.export.as_bytes();
    let (h, flavors) = m.mount(path)?;
    check(!h.is_empty() && h.len() <= 64, || {
        format!("handle of {} bytes", h.len())
    })?;
    check(flavors.contains(&1), || {
        format!("flavors {flavors:?} without AUTH_SYS")
    })?;
    let mut out = vec![
        line("handle bytes", h.len()),
        line("flavors", format!("{flavors:?}")),
    ];
    let listed = m.dump()?;
    check(listed.iter().any(|(_, d)| d == path), || {
        format!("DUMP after MNT does not list {}", c.t.export)
    })?;
    out.push(line("DUMP entries", listed.len()));
    let ex = m.export()?;
    check(
        ex.iter()
            .any(|(d, _)| path.starts_with(d) || d.eq_ignore_ascii_case(path)),
        || "EXPORT does not list the export".into(),
    )?;
    for (d, g) in &ex {
        out.push(format!(
            "export {} groups {:?}",
            String::from_utf8_lossy(d),
            g.iter()
                .map(|x| String::from_utf8_lossy(x).into_owned())
                .collect::<Vec<_>>()
        ));
    }
    m.umnt(path)?;
    let after = m.dump()?;
    check(
        after.len() < listed.len() || !after.iter().any(|(_, d)| d == path),
        || "DUMP after UMNT still lists the mount".into(),
    )?;
    out.push(line("DUMP entries after UMNT", after.len()));
    Ok(out)
}

fn not_exported(c: &Ctx) -> Lines {
    let mut m = Mount {
        c: c.client(c.mount_addr()?),
    };
    let mut out = Vec::new();
    for p in ["/nfstest-not-exported", "/"] {
        if c.t.export == p {
            continue;
        }
        match mnt_split(m.mnt(p.as_bytes())?) {
            Ok(_) => return fail(format!("MNT {p} was taken")),
            Err(s @ (mountstat3::MNT3ERR_ACCES | mountstat3::MNT3ERR_NOENT)) => {
                out.push(line(&format!("MNT {p}"), s));
            }
            Err(s) => return fail(format!("MNT {p}: {s}")),
        }
    }
    let missing = format!("{}/nfstest-no-such-dir", c.t.export.trim_end_matches('/'));
    match mnt_split(m.mnt(missing.as_bytes())?) {
        Err(s @ mountstat3::MNT3ERR_NOENT) => out.push(line("MNT of a missing directory", s)),
        Err(s) => return fail(format!("MNT {missing}: expected MNT3ERR_NOENT, got {s}")),
        Ok(_) => return fail(format!("MNT {missing} was taken")),
    }
    Ok(out)
}
