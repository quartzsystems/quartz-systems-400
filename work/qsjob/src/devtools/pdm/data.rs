// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the lists show, through the storage service: libraries,
//! objects, members; the user's defaults and option file.

use pdm::subset::{Subset, stamp_date};
use pdm::{Defaults, Item};
use stg::Client;

use crate::console::host::Host;

/// A library list part's libraries with their place: `SYS`, `PRD`,
/// `CUR`, `USR`.
fn library_list(host: &Host) -> Vec<(String, &'static str)> {
    let l = host.attrs().libl;
    let mut out: Vec<(String, &'static str)> = Vec::new();
    out.extend(l.sys.iter().map(|n| (n.clone(), "SYS")));
    out.extend(
        l.prd
            .iter()
            .filter(|n| !n.is_empty())
            .map(|n| (n.clone(), "PRD")),
    );
    out.extend(l.cur.iter().map(|n| (n.clone(), "CUR")));
    out.extend(l.usr.iter().map(|n| (n.clone(), "USR")));
    out
}

/// The job's current library.
pub fn curlib(host: &Host) -> Option<String> {
    host.attrs().libl.cur
}

/// The libraries a `WRKLIBPDM` value names, with their type as the list
/// shows it (`*PROD-SYS` on a library list, `*PROD` otherwise).
pub fn libraries(host: &Host, spec: &str) -> Vec<Item> {
    let spec = spec.trim().to_ascii_uppercase();
    let kind = |lib: &str| -> (String, String) {
        let o = host
            .link
            .borrow_mut()
            .with(|l| l.object("QSYS".into(), lib.into(), "*LIB".into()));
        let ty = o
            .as_ref()
            .map(|o| o.attr.trim().to_string())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "PROD".into());
        (format!("*{ty}"), o.map(|o| o.text).unwrap_or_default())
    };
    let from_list = |parts: &[&str]| -> Vec<Item> {
        library_list(host)
            .into_iter()
            .filter(|(_, p)| parts.contains(p))
            .map(|(n, p)| {
                let (ty, text) = kind(&n);
                Item {
                    library: n.clone(),
                    name: n,
                    ty: format!("{ty}-{p}"),
                    text,
                    ..Item::default()
                }
            })
            .collect()
    };
    match spec.as_str() {
        "*LIBL" => from_list(&["SYS", "PRD", "CUR", "USR"]),
        "*USRLIBL" => from_list(&["CUR", "USR"]),
        "*CURLIB" => from_list(&["CUR"]),
        _ => {
            let (pattern, user_only) = match spec.as_str() {
                "*ALL" | "" => ("*ALL".to_string(), false),
                "*ALLUSR" => ("*ALL".to_string(), true),
                other => (other.to_string(), false),
            };
            let mut libs: Vec<Item> = host
                .link
                .borrow_mut()
                .with(|l| l.list_objects("QSYS".into(), pattern.clone(), "*LIB".into()))
                .into_iter()
                .filter(|o| !user_only || !o.name.starts_with('Q') || o.name == "QGPL")
                .map(|o| Item {
                    library: o.name.clone(),
                    name: o.name,
                    ty: format!(
                        "*{}",
                        if o.attr.trim().is_empty() {
                            "PROD"
                        } else {
                            o.attr.trim()
                        }
                    ),
                    text: o.text,
                    ..Item::default()
                })
                .collect();
            libs.sort_by(|a, b| a.name.cmp(&b.name));
            libs
        }
    }
}

/// A library's objects of a subset, sorted by name and type.
pub fn objects(host: &Host, lib: &str, subset: &Subset) -> Vec<Item> {
    let name = if subset.name.trim().is_empty() {
        "*ALL".to_string()
    } else {
        subset.name.trim().to_ascii_uppercase()
    };
    let ty = if subset.ty.trim().is_empty() {
        "*ALL".to_string()
    } else {
        subset.ty.trim().to_ascii_uppercase()
    };
    let mut out: Vec<Item> = host
        .link
        .borrow_mut()
        .with(|l| l.list_objects(lib.into(), name, ty))
        .into_iter()
        .map(|o| Item {
            library: o.lib,
            name: o.name,
            ty: o.ty,
            attr: o.attr,
            text: o.text,
            ..Item::default()
        })
        .filter(|i| subset.keeps(i, 0))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.ty.cmp(&b.ty)));
    out
}

/// A member with the dates the list shows.
#[derive(Clone, Debug, Default)]
pub struct Member {
    /// The entry.
    pub item: Item,
    /// Changed, `YYYYMMDD`.
    pub changed: u32,
}

/// The library a file is found in through `lib` (`*LIBL`, `*CURLIB`
/// or a name).
pub fn file_library(host: &Host, lib: &str, file: &str) -> Option<String> {
    host.link
        .borrow_mut()
        .with(|l| l.file(lib.into(), file.into()))
        .map(|f| f.lib)
}

/// A file's members of a subset, sorted by name.
pub fn members(host: &Host, lib: &str, file: &str, subset: &Subset) -> Vec<Member> {
    let mut out: Vec<Member> = host
        .link
        .borrow_mut()
        .with(|l| l.members(lib.into(), file.into()))
        .into_iter()
        .map(|m| Member {
            changed: stamp_date(&m.changed),
            item: Item {
                library: lib.to_string(),
                file: file.to_string(),
                name: m.name,
                ty: m.srctype,
                text: m.text,
                ..Item::default()
            },
        })
        .filter(|m| subset.keeps(&m.item, m.changed))
        .collect();
    out.sort_by(|a, b| a.item.name.cmp(&b.item.name));
    out
}

/// The user's defaults, as stored or the shipped ones.
pub fn defaults(host: &Host) -> Defaults {
    let text = host.link.borrow_mut().with(Client::pdm_defaults);
    Defaults::parse(&text)
}

/// Store the user's defaults; the message when it failed.
pub fn save_defaults(host: &Host, d: &Defaults) -> Result<(), String> {
    host.link
        .borrow_mut()
        .with(|l| l.pdm_defaults_put(d.to_text()))
}

/// The user-defined options in the defaults' option file; the shipped
/// set when the shared file does not exist yet.
pub fn user_options(host: &Host, d: &Defaults) -> Vec<pdm::uopt::UserOption> {
    let got = host.link.borrow_mut().with(|l| {
        l.pdm_options(
            d.option_lib.clone(),
            d.option_file.clone(),
            d.option_member.clone(),
        )
    });
    match got {
        Ok(Some(lines)) => pdm::uopt::parse(&lines.join("\n")),
        Ok(None) if d.option_lib == "QGPL" && d.option_file == "QAUOOPT" => pdm::uopt::shipped(),
        _ => Vec::new(),
    }
}

/// Write the user-defined options back.
pub fn save_user_options(
    host: &Host,
    d: &Defaults,
    opts: &[pdm::uopt::UserOption],
) -> Result<(), String> {
    let lines: Vec<String> = pdm::uopt::to_text(opts)
        .lines()
        .map(str::to_string)
        .collect();
    host.link.borrow_mut().with(|l| {
        l.pdm_options_put(
            d.option_lib.clone(),
            d.option_file.clone(),
            d.option_member.clone(),
            lines,
        )
    })
}

/// A member's source lines; `None` when it cannot be read.
pub fn source(host: &Host, lib: &str, file: &str, member: &str) -> Option<Vec<String>> {
    host.link
        .borrow_mut()
        .with(|l| l.pdm_read(lib.into(), file.into(), member.into()))
        .ok()
        .flatten()
}
