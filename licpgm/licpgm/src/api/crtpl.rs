// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZCRTPL (Create Product Load) and QSZDLTPL (Delete Product Load).

use crate::api::crtpd::{qual20, store_error};
use crate::api::{ApiError, blank_at, char_at};
use crate::id::{self, LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::definition::{RegType, Registration};
use crate::model::load::{FolderEntry, LibEntry, LoadKey, ProductLoad};
use crate::model::state::LoadState;
use crate::store::{ProductStore, find_definition};

/// QSZCRTPL's parameters, read.
#[derive(Clone, Debug)]
pub struct CrtPl<'a> {
    /// Product load name, `Char(10)`.
    pub name: &'a [u8],
    /// Product load information, `Char(87)`.
    pub info: &'a [u8],
    /// Secondary language library, `Char(10)`.
    pub secondary_lib: &'a [u8],
    /// Principal library information, `Char(30)`.
    pub principal: &'a [u8],
    /// Additional libraries, `Char(30)` each.
    pub additional: &'a [u8],
    /// How many.
    pub n_additional: i32,
    /// Preoperation exit programs, `Char(20)` each.
    pub pre_exits: &'a [u8],
    /// How many.
    pub n_pre_exits: i32,
    /// Folders, `Char(126)` each.
    pub folders: &'a [u8],
    /// How many.
    pub n_folders: i32,
    /// Text description, `Char(50)`.
    pub text: &'a [u8],
    /// Public authority, `Char(10)`.
    pub public_aut: &'a [u8],
    /// Optional group 1: number of directories (their list's format is
    /// not read here; give 0 or leave it out).
    pub n_directories: Option<i32>,
}

/// Check a load against its definition and create it in the library
/// of its principal development library.
pub fn create_load(
    store: &mut dyn ProductStore,
    at: &QualName,
    l: &ProductLoad,
) -> Result<(), ApiError> {
    let k = &l.key;
    let Some((dq, d)) = find_definition(&*store, &k.product, k.release) else {
        return Err(ApiError::new("CPF0C4A")
            .text(k.product.as_str(), 7)
            .text(&id::release_text(k.release), 6));
    };
    let opt = d
        .option(k.option)
        .ok_or_else(|| ApiError::new("CPF0C8A").text(&k.option.api(), 4))?;
    let ok = match k.load_type {
        LoadType::Code => opt.feature == k.load_id,
        // *IBMLNG takes any language the system may have.
        LoadType::Lng => d
            .languages_of(k.option, std::slice::from_ref(&k.load_id))
            .contains(&k.load_id),
    };
    if !ok {
        return Err(ApiError::new("CPF0C84").text(k.load_id.as_str(), 4));
    }
    let lib = if at.lib == "*PRDDFN" {
        dq.lib.clone()
    } else {
        at.lib.clone()
    };
    let at = QualName::new(&at.name, &lib);
    if !store.library_exists(&lib) {
        return Err(ApiError::new("CPF9810").text(&lib, 10));
    }
    store.create_load(&at, l).map_err(|e| store_error(&e))
}

fn lib_entry(b: &[u8], dvl_of_prddfn: &str, is_lng: bool) -> Result<LibEntry, ApiError> {
    let dev = char_at(b, 0, 10);
    let dev = match dev.as_str() {
        "*PRDDFN" => dvl_of_prddfn.to_string(),
        "*CODE" if is_lng => dvl_of_prddfn.to_string(),
        n if id::is_name(n) => n.to_string(),
        n => return Err(ApiError::new("CPF3C29").text(n, 10)),
    };
    let primary = match char_at(b, 10, 10).as_str() {
        "*DVLLIB" | "" => dev.clone(),
        "*CODE" if is_lng => dev.clone(),
        n if id::is_name(n) => n.to_string(),
        n => return Err(ApiError::new("CPF3C29").text(n, 10)),
    };
    let pe = char_at(b, 20, 10);
    let mut e = LibEntry::new(&dev);
    e.primary.clone_from(&primary);
    e.installed = primary;
    e.post_exit = (pe != "*NONE" && !pe.is_empty()).then_some(pe);
    Ok(e)
}

/// QSZCRTPL over its parameter bytes.
pub fn qszcrtpl(store: &mut dyn ProductStore, p: &CrtPl<'_>) -> Result<(), ApiError> {
    let name = char_at(p.name, 0, 10);
    if !id::is_name(&name) {
        return Err(ApiError::new("CPF3C29").text(&name, 10));
    }
    let i = p.info;
    if i.len() < 87 || p.principal.len() < 30 {
        return Err(ApiError::new("CPF24B4"));
    }
    let pid = char_at(i, 0, 7);
    let product: ProductId = pid
        .parse()
        .map_err(|_| ApiError::new("CPF0CB2").text(&pid, 7))?;
    let rs = char_at(i, 7, 6);
    let release = id::parse_release(&rs).map_err(|_| ApiError::new("CPF358A").text(&rs, 6))?;
    let os = char_at(i, 13, 4);
    let option: OptionNo = os
        .parse()
        .map_err(|_| ApiError::new("CPF0C8A").text(&os, 4))?;
    let lt = char_at(i, 17, 10);
    let load_type = LoadType::parse(&lt).map_err(|_| ApiError::new("CPF0C84").text(&lt, 10))?;
    let ls = char_at(i, 27, 8);
    let load_id = match (load_type, ls.as_str()) {
        (LoadType::Code, "*CODEDFT") => LoadId::feature(LoadId::CODE_DEFAULT).expect("5001"),
        (t, s) => LoadId::for_type(t, s).map_err(|_| ApiError::new("CPF0C84").text(s, 8))?,
    };
    let rt = char_at(i, 35, 10);
    let registration = match rt.as_str() {
        "*PRDDFN" => None,
        "*PHONE" | "*CUSTOMER" => Some(Registration {
            kind: if rt == "*PHONE" {
                RegType::Phone
            } else {
                RegType::Customer
            },
            value: char_at(i, 45, 14),
        }),
        _ => return Err(ApiError::new("CPF0C86")),
    };
    let mt = char_at(i, 59, 10);
    let min_target = match mt.as_str() {
        "" | "*CURRENT" | "*CODE" | "*BASECODE" => None,
        "*PRV" => None,
        r => Some(id::parse_release(r).map_err(|_| ApiError::new("CPF358A").text(r, 10))?),
    };
    if !blank_at(i, 69, 18) {
        return Err(ApiError::new("CPF0CB3"));
    }
    let Some((dq, _)) = find_definition(&*store, &product, release) else {
        return Err(ApiError::new("CPF0C4A").text(&pid, 7).text(&rs, 6));
    };
    let is_lng = load_type == LoadType::Lng;
    let mut libraries = vec![lib_entry(p.principal, &dq.lib, is_lng)?];
    let n = usize::try_from(p.n_additional).unwrap_or(usize::MAX);
    if n > 1000 || p.additional.len() < n * 30 {
        return Err(ApiError::new("CPF24B4"));
    }
    for j in 0..n {
        libraries.push(lib_entry(
            &p.additional[j * 30..(j + 1) * 30],
            &dq.lib,
            is_lng,
        )?);
    }
    let n = usize::try_from(p.n_pre_exits).unwrap_or(usize::MAX);
    if n > 1000 || p.pre_exits.len() < n * 20 {
        return Err(ApiError::new("CPF24B4"));
    }
    let pre_exits = (0..n)
        .map(|j| {
            let e = &p.pre_exits[j * 20..(j + 1) * 20];
            QualName::new(&char_at(e, 0, 10), &char_at(e, 10, 10))
        })
        .collect();
    let n = usize::try_from(p.n_folders).unwrap_or(usize::MAX);
    if n > 100 || p.folders.len() < n * 126 {
        return Err(ApiError::new("CPF24B4"));
    }
    if n > 0 && p.n_directories.unwrap_or(0) > 0 {
        return Err(ApiError::new("CPF24B4"));
    }
    if p.n_directories.unwrap_or(0) != 0 {
        // MIGRATE(phase11): read the directory list (SDIR0100) once its
        // layout is confirmed; create_load takes directories typed.
        return Err(ApiError::new("CPF3C21").text("SDIR0100", 8));
    }
    let folders = (0..n)
        .map(|j| {
            let e = &p.folders[j * 126..(j + 1) * 126];
            let dev = char_at(e, 0, 63);
            let pr = char_at(e, 63, 63);
            let primary = if pr == "*DVLFLR" || pr.is_empty() {
                dev.clone()
            } else {
                pr
            };
            FolderEntry {
                dev,
                installed: primary.clone(),
                primary,
            }
        })
        .collect();
    let principal_lib = libraries[0].dev.clone();
    let key = LoadKey {
        product,
        release,
        option,
        load_type,
        load_id,
    };
    let mut l = ProductLoad::new(key, &principal_lib, &char_at(p.text, 0, 50));
    l.registration = registration;
    l.min_target = min_target;
    let sl = char_at(p.secondary_lib, 0, 10);
    l.secondary_lib = (!sl.is_empty() && sl != "*NONE").then_some(sl);
    l.libraries = libraries;
    l.pre_exits = pre_exits;
    l.folders = folders;
    l.public_aut = char_at(p.public_aut, 0, 10);
    l.state = LoadState::Created;
    create_load(store, &QualName::new(&name, &principal_lib), &l)
}

/// QSZDLTPL: delete a product load. An installed load is refused with
/// `CPF0C52`; DLTLICPGM removes those.
pub fn qszdltpl(store: &mut dyn ProductStore, qual: &[u8]) -> Result<(), ApiError> {
    let at = qual20(qual, "")?;
    let Some((_, l)) = store.loads().into_iter().find(|(q, _)| q == &at) else {
        return Err(ApiError::new("CPF2105")
            .text(&at.name, 10)
            .text(&at.lib, 10));
    };
    if l.state.is_installed() {
        return Err(ApiError::new("CPF0C52")
            .text(&at.name, 10)
            .text(&at.lib, 10));
    }
    store.delete_load(&at).map_err(|e| store_error(&e))
}
