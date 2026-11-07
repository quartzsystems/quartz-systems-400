// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `obj fill` and `obj verify`: user spaces with a known pattern, kept,
//! so a disk test can put data on every unit and read it back later.

use qsname::Name;
use store::obj::{ops, types, usrspc};

use super::objects::{params, target};
use crate::ipl::system::System;

/// User spaces per transaction.
const BATCH: u64 = 8;
/// Checkpoint after this many batches.
const CHECKPOINT_EVERY: u64 = 16;

/// The byte at `at` of space `i`.
fn pattern(i: u64, at: u64) -> u8 {
    u8::try_from((i * 31 + at * 7 + at / 4096) % 251).expect("below 251")
}

fn space_name(i: u64) -> Name {
    Name::parse(&format!("F{i:08}")).expect("valid")
}

fn contents(i: u64, bytes: u64) -> Vec<u8> {
    (0..bytes).map(|at| pattern(i, at)).collect()
}

fn args(n: &str, kib: &str) -> Result<(u64, u64), String> {
    let n: u64 = n.parse().map_err(|_| format!("{n}: not a count"))?;
    let kib: u64 = kib
        .parse()
        .map_err(|_| format!("{kib}: not a size in KiB"))?;
    if n == 0 || kib == 0 {
        return Err("N and KIB must be more than 0".into());
    }
    Ok((n, kib * 1024))
}

/// `obj fill LIB N KIB`: the library made if missing, then `N` user
/// spaces of `KIB` KiB each, a pattern in each.
pub fn fill(system: &mut System, lib: &str, n: &str, kib: &str) -> Result<String, String> {
    let (n, bytes) = args(n, kib)?;
    let (store, root) = target(system, "QSYS")?;
    let name = Name::parse(lib).map_err(|e| e.to_string())?;
    let lib_id = {
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        let id = match ops::find_library(&mut txn, root, name) {
            Ok(f) => f.id,
            Err(_) => ops::create(
                &mut txn,
                &params(root, name, types::LIB, "Filled by the console"),
            )
            .map_err(|e| e.to_string())?,
        };
        txn.commit().map_err(|e| e.to_string())?;
        id
    };
    let mut made = 0;
    while made < n {
        let end = (made + BATCH).min(n);
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        for i in made..end {
            let p = params(lib_id, space_name(i), types::USRSPC, "Fill");
            let id = usrspc::create(&mut txn, &p, bytes, false, 0)
                .map_err(|e| format!("create {i}: {e}"))?;
            usrspc::write(&mut txn, id, 0, &contents(i, bytes))
                .map_err(|e| format!("write {i}: {e}"))?;
        }
        txn.commit().map_err(|e| e.to_string())?;
        if (end / BATCH).is_multiple_of(CHECKPOINT_EVERY) {
            store.checkpoint().map_err(|e| e.to_string())?;
        }
        made = end;
    }
    store.checkpoint().map_err(|e| e.to_string())?;
    Ok(format!(
        "filled {lib}: {n} user spaces of {} KiB\n",
        bytes / 1024
    ))
}

/// `obj verify LIB N KIB`: every space `fill` made reads back as made.
pub fn verify(system: &mut System, lib: &str, n: &str, kib: &str) -> Result<String, String> {
    let (n, bytes) = args(n, kib)?;
    let (store, root) = target(system, "QSYS")?;
    let name = Name::parse(lib).map_err(|e| e.to_string())?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let lib_id = ops::find_library(&mut txn, root, name)
        .map_err(|e| e.to_string())?
        .id;
    let l = store::obj::library::Library::load(&mut txn, lib_id).map_err(|e| e.to_string())?;
    let mut bad = Vec::new();
    for i in 0..n {
        let Some(id) = l
            .lookup(&mut txn, types::USRSPC, space_name(i))
            .map_err(|e| e.to_string())?
        else {
            bad.push(format!("{} missing", space_name(i)));
            continue;
        };
        let usize_bytes = usize::try_from(bytes).map_err(|e| e.to_string())?;
        match usrspc::read(&mut txn, id, 0, usize_bytes) {
            Ok(got) if got == contents(i, bytes) => {}
            Ok(_) => bad.push(format!("{} differs", space_name(i))),
            Err(e) => bad.push(format!("{}: {e}", space_name(i))),
        }
    }
    txn.abort();
    if bad.is_empty() {
        Ok(format!("verified {lib}: {n} user spaces intact\n"))
    } else {
        let first: Vec<String> = bad.iter().take(5).cloned().collect();
        Ok(format!(
            "verify {lib}: {} of {n} bad: {}\n",
            bad.len(),
            first.join("; ")
        ))
    }
}
