// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! National language libraries: `QSYS2924`, the primary language's, at
//! every first IPL, and a secondary language's `QSYS29xx` (`INSLNG`)
//! with its message files and its sign-on display files.

use qsname::Name;
use store::obj::{ObjectId, types};
use store::txn::Txn;

use crate::display::{ATTR, compile};

/// The display files a secondary language library carries.
const DISPLAY_FILES: &[&str] = &["QDSIGNON", "QDSIGNON2"];

/// A shipped message's first-level text.
fn text(id: &str) -> String {
    msgf::builtin()
        .into_iter()
        .find(|f| f.name == "QSTXTMSG")
        .and_then(|f| f.find(id).map(|d| d.text.clone()))
        .unwrap_or_else(|| id.to_string())
}

fn ctx(stamp: &wm::obj::Stamp) -> msgf::store::Ctx {
    msgf::store::Ctx {
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    }
}

/// `QSYS2924` when absent; how many objects were made.
pub fn install_primary(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = format!("QSYS{}", ccsid::nls::PRIMARY);
    msgf::store::ensure_library(txn, &ctx(stamp), qsys, &lib, &text("TXT0201"))
        .map(|(_, made)| usize::from(made))
        .map_err(|e| e.to_string())
}

/// Language `feature`'s library, message files and display files;
/// `replace` replaces those that exist. The library and the objects
/// created, or `TXT0203` when the system has no text for it.
pub fn install_language(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
    feature: u16,
    replace: bool,
) -> Result<(String, usize), String> {
    let lang = msgf::nls::language(feature).ok_or_else(|| format!("TXT0203\t{feature}"))?;
    let (lib, made) = msgf::store::install_language(txn, &ctx(stamp), qsys, &lang, replace)
        .map_err(|e| e.to_string())?;
    let mut n = made.len();
    for name in DISPLAY_FILES {
        let Some((_, dds)) = dspf::sys::FILES.iter().find(|(f, _)| f == name) else {
            continue;
        };
        let nm = Name::parse(name).map_err(|e| e.to_string())?;
        let found = store::obj::library::Library::load(txn, lib)
            .and_then(|l| l.lookup(txn, types::FILE, nm))
            .map_err(|e| e.to_string())?;
        if found.is_some() && !replace {
            continue;
        }
        if let Some(id) = found {
            store::obj::ops::delete(txn, store::obj::JobHandle(0), id)
                .map_err(|e| e.to_string())?;
        }
        let (src, _) = dspf::Source::from_text(&lang.translate_dds(dds));
        let (payload, _, _) = compile(&src, &dspf::MapRefs::default(), name)?;
        let desc = format!("{name} {}", lang.marker);
        wm::obj::create_attr(txn, lib, (nm, types::FILE, ATTR), &desc, stamp, &payload)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
        n += 1;
    }
    Ok((lang.library(), n))
}
