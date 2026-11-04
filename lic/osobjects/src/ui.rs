// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel groups and menus: `*PNLGRP` objects hold a compiled UIM
//! payload, `*MENU` objects their attributes. The shipped ones (the
//! system's panels, the command help, the command menus and the task
//! menus) are compiled and created in `QSYS` when missing, and replaced
//! when the shipped source changed (an upgrade's new help). The search
//! index `QSYS/QHSS1` holds a topic for every help module of them and is
//! made again when any of them was.

use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

/// The object type code of `*PNLGRP`.
#[must_use]
pub fn pnlgrp_type() -> u16 {
    types::type_by_name("*PNLGRP").map_or(0x0012, |t| t.code)
}

/// The object type code of `*MENU`.
#[must_use]
pub fn menu_type() -> u16 {
    types::type_by_name("*MENU").map_or(0x0011, |t| t.code)
}

/// Compile UIM text into a panel group payload, or the listing.
pub fn compile(text: &str, name: &str, lib: &str) -> Result<(Vec<u8>, String), String> {
    let opts = uim::CompileOpts {
        name: Some(name.to_string()),
        library: Some(lib.to_string()),
        ..uim::CompileOpts::default()
    };
    let c = uim::compile(text, &uim::NoImbeds, &uim::NoImports, &opts);
    match c.group {
        Some(group) if c.ok() => {
            let p = uim::Payload {
                name: name.to_string(),
                library: lib.to_string(),
                group,
                source: c.source.lines().to_vec(),
            };
            Ok((uim::payload::encode(&p), c.listing))
        }
        _ => Err(c.listing),
    }
}

/// Create (or replace) a panel group object with `payload`.
pub fn create_pnlgrp(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    text: &str,
    stamp: &wm::obj::Stamp,
    payload: &[u8],
    replace: bool,
) -> Result<ObjectId, String> {
    replace_existing(txn, lib, name, pnlgrp_type(), replace)?;
    wm::obj::create_attr(txn, lib, (name, pnlgrp_type(), "UIM"), text, stamp, payload)
        .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9801")))
}

/// Create (or replace) a menu object.
pub fn create_menu(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    attrs: &menus::MenuAttrs,
    stamp: &wm::obj::Stamp,
    replace: bool,
) -> Result<ObjectId, String> {
    replace_existing(txn, lib, name, menu_type(), replace)?;
    wm::obj::create_attr(
        txn,
        lib,
        (name, menu_type(), attrs.kind.text().trim_start_matches('*')),
        &attrs.text,
        stamp,
        &attrs.encode(),
    )
    .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9801")))
}

/// Delete an object of the type in `lib` when it exists; refuse when
/// it exists and `replace` is off.
fn replace_existing(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    type_code: u16,
    replace: bool,
) -> Result<(), String> {
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    if let Some(id) = l.lookup(txn, type_code, name).map_err(|e| e.to_string())? {
        if !replace {
            return Err(format!("CPF2112 object {name} already exists"));
        }
        ops::delete(txn, store::obj::JobHandle(0), id).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The search index of the system's help.
pub const QHSS1: &str = "QHSS1";

/// The object type code of `*SCHIDX`.
#[must_use]
pub fn schidx_type() -> u16 {
    types::type_by_name("*SCHIDX").map_or(0x0023, |t| t.code)
}

/// Whether the stored panel group `id` was compiled from `text`.
fn same_source(txn: &mut Txn<'_>, id: ObjectId, text: &str) -> bool {
    let Ok(bytes) = wm::obj::read(txn, id) else {
        return false;
    };
    let Ok(p) = uim::payload::decode(&bytes) else {
        return false;
    };
    let stored = p.source.iter().map(|l| l.text.as_str());
    // Trimmed as the compiler trims a source line.
    let shipped = text
        .lines()
        .map(|l| l.trim_end_matches(|c: char| c == ' ' || c.is_ascii_control()));
    stored.eq(shipped)
}

/// A shipped panel group into `QSYS`: created when missing, replaced
/// when its source changed; whether it was made.
fn ship_group(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
    (name, text, what): (&str, &str, &str),
) -> Result<bool, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
    let have = lib.lookup(txn, pnlgrp_type(), n).map_err(|e| e.to_string())?;
    if have.is_some_and(|id| same_source(txn, id, text)) {
        return Ok(false);
    }
    let (payload, _) = compile(text, name, "QSYS")
        .map_err(|l| format!("shipped panel group {name} does not compile:\n{l}"))?;
    create_pnlgrp(
        txn,
        qsys,
        n,
        &format!("Shipped {what} {name}"),
        stamp,
        &payload,
        have.is_some(),
    )?;
    Ok(true)
}

/// A topic for every help module of the shipped groups: its own
/// `:ISCH.` topics, else its title with the title's words as roots.
#[must_use]
pub fn qhss1_entries() -> Vec<(String, Vec<String>, String, String)> {
    let mut out = Vec::new();
    for (name, text) in uim::shipped::FILES.iter().chain(menus::shipped::FILES) {
        let c = uim::compile(text, &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        let Some(g) = c.group else {
            continue;
        };
        let qualified = format!("QSYS/{name}");
        let topics = uim::engine::IndexEntry::of_group(&g, &qualified);
        for h in &g.helps {
            let own: Vec<_> = topics.iter().filter(|t| t.module == h.name).collect();
            if !own.is_empty() {
                out.extend(
                    own.into_iter()
                        .map(|t| (t.title.clone(), t.roots.clone(), t.pnlgrp.clone(), t.module.clone())),
                );
                continue;
            }
            let title = uim::model::Span::plain(&h.title).trim().to_string();
            let roots: Vec<String> = title
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .map(str::to_ascii_uppercase)
                .filter(|w| !matches!(w.as_str(), "THE" | "AND" | "FOR" | "HELP" | "WITH"))
                .collect();
            if title.is_empty() || roots.is_empty() {
                continue;
            }
            out.push((title, roots, qualified.clone(), h.name.clone()));
        }
    }
    out
}

/// `QSYS/QHSS1` made again from the shipped groups.
fn ship_qhss1(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<(), String> {
    let n = Name::parse(QHSS1).map_err(|e| e.to_string())?;
    replace_existing(txn, qsys, n, schidx_type(), true)?;
    let entries = qhss1_entries();
    wm::obj::create_attr(
        txn,
        qsys,
        (n, schidx_type(), ""),
        "System help search index",
        stamp,
        &ipc::encode(&entries),
    )
    .map(|_| ())
    .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF6E11")))
}

/// The shipped panel groups and menus missing from `QSYS` or changed,
/// compiled from their UIM and created (a `*MENU` of the same name over
/// each), and `QHSS1` when any was; how many objects were made.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let mut made = 0;
    for (name, text) in uim::shipped::FILES {
        if ship_group(txn, qsys, stamp, (name, text, "panel group"))? {
            made += 1;
        }
    }
    for (name, text) in menus::shipped::FILES {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        if ship_group(txn, qsys, stamp, (name, text, "menu"))? {
            made += 1;
        }
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let have_menu = lib
            .lookup(txn, menu_type(), n)
            .map_err(|e| e.to_string())?
            .is_some();
        if !have_menu {
            let attrs = menus::MenuAttrs::uim(
                menus::Qualified::qsys(name),
                &format!("Shipped menu {name}"),
            );
            create_menu(txn, qsys, n, &attrs, stamp, false)?;
            made += 1;
        }
    }
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let idx = Name::parse(QHSS1).map_err(|e| e.to_string())?;
    if made > 0 || lib.lookup(txn, schidx_type(), idx).map_err(|e| e.to_string())?.is_none() {
        ship_qhss1(txn, qsys, stamp)?;
        made += 1;
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    #[test]
    fn qhss1_holds_every_kind_of_help() {
        let e = super::qhss1_entries();
        assert!(e.len() > 5000, "{}", e.len());
        assert!(e.iter().any(|t| t.2 == "QSYS/QHWMCMD" && t.3 == "ADDLIBLE/LIB"));
        assert!(e.iter().any(|t| t.2 == "QSYS/QUIWRKLIB" && t.3 == "WRKLIB"));
        assert!(e.iter().all(|t| !t.1.is_empty()));
    }
}
