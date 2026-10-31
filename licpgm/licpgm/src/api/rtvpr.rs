// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZRTVPR, Retrieve Product Information: formats `PRDR0100` to
//! `PRDR0800` (`docs/LICPGM.md` "QSZRTVPR").
//!
//! Parameters: receiver, length of receiver, format name, product
//! information, error code, and optionally the product information
//! format (`PRDI0100` default, `PRDI0200`).

use release::Release;

use crate::api::{ApiError, Rcv, bin4_at, char_at};
use crate::directory::requirements_met;
use crate::id::{self, LoadId, LoadType, OptionNo, ProductId};
use crate::model::definition::{ProductDefinition, RegType};
use crate::model::load::{LoadKey, ProductLoad};
use crate::model::state::LoadState;
use crate::store::ProductStore;
use crate::system::SystemInfo;

/// Where the format-specific part starts: the end of `PRDR0100`.
pub const ADDITIONAL: usize = 108;

/// The product information parameter, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductInfo {
    /// Product ID or `*OPSYS`.
    pub product: String,
    /// Release, `*CUR`, `*ONLY` or `*PRV`.
    pub release: String,
    /// `0000`..`0099`.
    pub option: String,
    /// `*CODE` or a load ID.
    pub load_id: String,
    /// `PRDI0200`'s CCSID for returned names; 0 is the job's.
    pub ccsid: i32,
}

impl ProductInfo {
    /// Read `PRDI0100` (27 bytes) or `PRDI0200` (36 bytes and more).
    pub fn read(b: &[u8], format: Option<&str>) -> Result<Self, ApiError> {
        let f = format.unwrap_or("PRDI0100");
        let need = match f {
            "PRDI0100" => 27,
            "PRDI0200" => 36,
            _ => return Err(ApiError::new("CPF3C21").text(f, 8)),
        };
        if b.len() < need || (f == "PRDI0200" && usize::try_from(bin4_at(b, 28)).unwrap_or(0) < 36)
        {
            return Err(ApiError::new("CPF3C1D"));
        }
        Ok(Self {
            product: char_at(b, 0, 7),
            release: char_at(b, 7, 6),
            option: char_at(b, 13, 4),
            load_id: char_at(b, 17, 10),
            ccsid: if f == "PRDI0200" { bin4_at(b, 32) } else { 0 },
        })
    }
}

/// The load a request names, resolved: the definition, the load (or
/// one synthesized in state `10` when the definition names it but no
/// object exists), and the product ID to echo.
struct Found {
    echo: String,
    def: ProductDefinition,
    load: ProductLoad,
    loads: Vec<ProductLoad>,
    defs: Vec<ProductDefinition>,
}

fn not_found(pi: &ProductInfo) -> ApiError {
    ApiError::new("CPF0C1F")
        .text(&pi.product, 7)
        .text(&pi.release, 6)
        .text(&pi.option, 4)
        .text(&pi.load_id, 10)
}

fn resolve(store: &dyn ProductStore, pi: &ProductInfo) -> Result<Found, ApiError> {
    let defs: Vec<ProductDefinition> = store.definitions().into_iter().map(|(_, d)| d).collect();
    let loads: Vec<ProductLoad> = store.loads().into_iter().map(|(_, l)| l).collect();
    let (want, echo) = if pi.product == "*OPSYS" {
        (ProductId::OS.to_string(), ProductId::OS.to_string())
    } else {
        (pi.product.clone(), pi.product.clone())
    };
    let mine: Vec<&ProductDefinition> = defs.iter().filter(|d| d.answers_to(&want)).collect();
    if mine.is_empty() {
        return Err(not_found(pi));
    }
    let option: OptionNo = if pi.option.is_empty() {
        OptionNo::BASE
    } else {
        pi.option.parse().map_err(|_| not_found(pi))?
    };
    let pid = mine[0].product.clone();
    let rel = match pi.release.as_str() {
        "*CUR" => loads
            .iter()
            .filter(|l| {
                l.key.product == pid && l.key.option == option && l.key.load_type == LoadType::Code
            })
            .filter(|l| l.state.is_installed())
            .map(|l| l.key.release)
            .max()
            .ok_or_else(|| not_found(pi))?,
        "*ONLY" => {
            let mut rels: Vec<Release> = loads
                .iter()
                .filter(|l| l.key.product == pid)
                .map(|l| l.key.release)
                .collect();
            rels.sort();
            rels.dedup();
            match rels.as_slice() {
                [r] => *r,
                [] => return Err(not_found(pi)),
                _ => return Err(ApiError::new("CPF0C1C").text(&pi.product, 7)),
            }
        }
        "*PRV" => {
            let cur = mine
                .iter()
                .map(|d| d.release)
                .max()
                .ok_or_else(|| not_found(pi))?;
            mine.iter()
                .map(|d| d.release)
                .filter(|r| r.m == 0 && (r.v, r.r) < (cur.v, cur.r))
                .max()
                .ok_or_else(|| not_found(pi))?
        }
        r => id::parse_release(r).map_err(|_| ApiError::new("CPF0C1D").text(r, 6))?,
    };
    let def = mine
        .iter()
        .find(|d| d.release == rel)
        .copied()
        .cloned()
        .ok_or_else(|| not_found(pi))?;
    let opt = def.option(option).ok_or_else(|| not_found(pi))?.clone();
    let (load_type, load_id) = match pi.load_id.as_str() {
        "*CODE" | "" => (LoadType::Code, opt.feature.clone()),
        s => {
            if let Ok(f) = LoadId::feature(s) {
                (LoadType::Code, f)
            } else {
                (LoadType::Lng, LoadId::nlv(s).map_err(|_| not_found(pi))?)
            }
        }
    };
    let key = LoadKey {
        product: def.product.clone(),
        release: rel,
        option,
        load_type,
        load_id: load_id.clone(),
    };
    let load = match loads.iter().find(|l| l.key == key) {
        Some(l) => l.clone(),
        None => {
            let named = match load_type {
                LoadType::Code => opt.feature == load_id,
                LoadType::Lng => def.languages_of(option, &[]).contains(&load_id),
            };
            if !named {
                return Err(not_found(pi));
            }
            let mut l = ProductLoad::new(key, "", &opt.text);
            l.libraries.clear();
            l.state = LoadState::Defined;
            l
        }
    };
    Ok(Found {
        echo,
        def,
        load,
        loads,
        defs,
    })
}

fn rel6(r: Release) -> String {
    id::release_text(r)
}

/// Fill the `PRDR0100` part.
fn base(f: &Found, sys: &SystemInfo, r: &mut Rcv, extended: bool) {
    let l = &f.load;
    let k = &l.key;
    r.bin4(0, 0);
    r.bin4(4, 0);
    r.bin4(8, 0);
    r.char(12, 7, &f.echo);
    r.char(19, 6, &rel6(k.release));
    r.char(25, 4, &k.option.api());
    r.char(29, 4, k.load_id.as_str());
    r.char(33, 10, k.load_type.name());
    r.char(43, 10, l.state.symbolic());
    r.char(
        53,
        10,
        if l.state.is_error() {
            "*ERROR"
        } else {
            "*NONE"
        },
    );
    r.char(63, 2, l.state.code());
    r.char(65, 1, if l.supported { "1" } else { "0" });
    let reg = l
        .registration
        .clone()
        .unwrap_or_else(|| f.def.registration.clone());
    let value = if reg.kind == RegType::OsSame {
        f.defs
            .iter()
            .find(|d| d.product.as_str() == ProductId::OS && !d.registration.value.is_empty())
            .map_or_else(|| sys.serial.clone(), |d| d.registration.value.clone())
    } else {
        reg.value.clone()
    };
    r.char(66, 2, reg.kind.code());
    r.char(68, 14, &value);
    r.blank(82, 2);
    r.bin4(
        84,
        if extended {
            i32::try_from(ADDITIONAL).unwrap_or(0)
        } else {
            0
        },
    );
    let lngs: Vec<&ProductLoad> = f
        .loads
        .iter()
        .filter(|x| {
            x.key.product == k.product
                && x.key.release == k.release
                && x.key.option == k.option
                && x.key.load_type == LoadType::Lng
                && x.state.is_installed()
        })
        .collect();
    let primary = lngs
        .iter()
        .find(|x| x.key.load_id == sys.primary_lng)
        .or_else(|| lngs.first())
        .map_or(String::new(), |x| x.key.load_id.to_string());
    r.char(88, 4, &primary);
    r.char(92, 6, &rel6(l.min_target.unwrap_or(f.def.release)));
    let opt = f.def.option(k.option);
    r.char(
        98,
        6,
        &rel6(opt.and_then(|o| o.min_base).unwrap_or(f.def.release)),
    );
    let met = if k.load_type == LoadType::Lng {
        '0'
    } else {
        f.loads
            .iter()
            .find(|x| {
                x.key.product == k.product
                    && x.key.option.is_base()
                    && x.key.load_type == LoadType::Code
                    && x.state.is_installed()
            })
            .map_or('0', |b| {
                requirements_met(&f.def, k.option, k.release, b.key.release)
            })
    };
    r.char(104, 1, &met.to_string());
    let is_os = f.def.product.as_str() == ProductId::OS;
    r.char(105, 3, if is_os { "L00" } else { "" });
}

fn prdr0200(f: &Found, r: &mut Rcv) {
    let l = &f.load;
    let a = ADDITIONAL;
    r.char(a, 10, l.secondary_lib.as_deref().unwrap_or(""));
    r.blank(a + 10, 2);
    r.bin4(a + 12, i32::try_from(l.libraries.len()).unwrap_or(0));
    let first = a + 20;
    r.bin4(
        a + 16,
        if l.libraries.is_empty() {
            0
        } else {
            i32::try_from(first).unwrap_or(0)
        },
    );
    let mut at = first;
    for (i, lib) in l.libraries.iter().enumerate() {
        let exits: &[crate::model::QualName] = if i == 0 { &l.pre_exits } else { &[] };
        let len = (68 + 10 * exits.len()).next_multiple_of(4);
        let next = if i + 1 == l.libraries.len() {
            0
        } else {
            at + len
        };
        r.bin4(at, i32::try_from(next).unwrap_or(0));
        r.char(at + 4, 10, &lib.primary);
        r.char(at + 14, 10, &lib.installed);
        r.char(at + 24, 10, lib.lib_type.name());
        r.char(at + 34, 10, &lib.aut);
        r.char(at + 44, 10, &lib.crt_aut);
        r.char(at + 54, 10, lib.post_exit.as_deref().unwrap_or("*NONE"));
        r.bin4(at + 64, i32::try_from(exits.len()).unwrap_or(0));
        for (j, e) in exits.iter().enumerate() {
            r.char(at + 68 + 10 * j, 10, &e.name);
        }
        r.align4();
        at += len;
    }
}

/// A count, record length and offset at 108, records from 120.
fn records(r: &mut Rcv, n: usize, len: usize) -> usize {
    let a = ADDITIONAL;
    r.bin4(a, i32::try_from(n).unwrap_or(0));
    r.bin4(a + 4, i32::try_from(len).unwrap_or(0));
    r.bin4(a + 8, i32::try_from(a + 12).unwrap_or(0));
    a + 12
}

fn prdr0300(f: &Found, r: &mut Rcv) {
    let fl = &f.load.folders;
    let mut at = records(r, fl.len(), 126);
    for x in fl {
        r.char(at, 63, &x.primary);
        r.char(at + 63, 63, &x.installed);
        at += 126;
    }
}

fn prdr0400(f: &Found, r: &mut Rcv) {
    let l = &f.load;
    let mut at = records(r, l.objects.len(), 30);
    for o in &l.objects {
        let lib = l
            .libraries
            .iter()
            .find(|e| e.primary == o.lib)
            .map_or(o.lib.as_str(), |e| e.installed.as_str());
        r.char(at, 10, &o.name);
        r.char(at + 10, 10, lib);
        r.char(at + 20, 10, &o.typ);
        at += 30;
    }
}

fn prdr0500(f: &Found, r: &mut Rcv) {
    let d = &f.def;
    let a = ADDITIONAL;
    r.char(a, 1, if d.allow_multiple_releases { "1" } else { "0" });
    match d.release_date {
        Some(dt) => {
            r.char(a + 1, 1, if dt.year >= 2000 { "1" } else { "0" });
            r.char(
                a + 2,
                6,
                &format!("{:02}{:02}{:02}", dt.year % 100, dt.month, dt.day),
            );
        }
        None => {
            r.blank(a + 1, 1);
            r.blank(a + 2, 6);
        }
    }
    let year = |y: Option<u16>| y.map(|y| y.to_string()).unwrap_or_default();
    r.char(a + 8, 4, &year(d.first_copyright));
    r.char(a + 12, 4, &year(d.current_copyright));
    let (mn, ml) = d
        .msgf
        .as_ref()
        .map_or(("", ""), |q| (q.name.as_str(), q.lib.as_str()));
    r.char(a + 16, 10, mn);
    r.char(a + 26, 10, ml);
    r.bin4(a + 36, i32::try_from(d.options.len()).unwrap_or(0));
    r.bin4(a + 40, 18);
    let first = a + 52;
    r.bin4(a + 44, i32::try_from(first).unwrap_or(0));
    r.char(a + 48, 1, if d.allow_mixed_releases { "1" } else { "0" });
    r.blank(a + 49, 3);
    let mut at = first;
    for o in &d.options {
        r.char(at, 4, &o.option.api());
        r.char(at + 4, 1, if o.dynamic_naming { "1" } else { "0" });
        r.char(at + 5, 7, &o.msg_id);
        r.char(at + 12, 6, &rel6(o.min_base.unwrap_or(d.release)));
        at += 18;
    }
}

fn prdr0600(f: &Found, sys: &SystemInfo, r: &mut Rcv) {
    let d = &f.def;
    let mut recs: Vec<(OptionNo, String)> = Vec::new();
    for o in &d.options {
        recs.push((o.option, o.feature.to_string()));
        for l in d.languages_of(o.option, &sys.languages) {
            recs.push((o.option, l.to_string()));
        }
    }
    let mut at = records(r, recs.len(), 8);
    for (o, id) in recs {
        r.char(at, 4, &o.api());
        r.char(at + 4, 4, &id);
        at += 8;
    }
}

fn prdr0700(f: &Found, r: &mut Rcv) {
    let mut rels: Vec<Release> = f
        .defs
        .iter()
        .filter(|d| d.product == f.def.product)
        .map(|d| d.release)
        .collect();
    rels.sort();
    rels.dedup();
    r.bin4(8, i32::try_from(rels.len()).unwrap_or(0));
    r.bin4(12, 0);
    for (i, rel) in rels.iter().enumerate() {
        r.char(16 + 6 * i, 6, &rel6(*rel));
    }
}

/// Encode a name in the requested CCSID; `None` when it cannot be.
fn encode_name(s: &str, ccsid: i32) -> Option<Vec<u8>> {
    match ccsid {
        0 | 37 => ccsid::to_ebcdic(s).ok(),
        1208 => Some(s.as_bytes().to_vec()),
        1200 | 13488 => Some(s.encode_utf16().flat_map(u16::to_be_bytes).collect()),
        _ => None,
    }
}

fn prdr0800(f: &Found, ccsid: i32, r: &mut Rcv) {
    let dirs = &f.load.directories;
    let a = ADDITIONAL;
    const ENTRY: usize = 52;
    let used = if matches!(ccsid, 0 | 37 | 1200 | 1208 | 13488) {
        ccsid
    } else {
        1200
    };
    let mut err = used != ccsid;
    r.bin4(a, i32::try_from(dirs.len()).unwrap_or(0));
    r.bin4(a + 4, i32::try_from(ENTRY).unwrap_or(0));
    let first = a + 20;
    r.bin4(a + 8, i32::try_from(first).unwrap_or(0));
    r.bin4(a + 12, if used == 0 { 37 } else { used });
    let mut data = first + ENTRY * dirs.len();
    for (i, d) in dirs.iter().enumerate() {
        let e = first + ENTRY * i;
        for (j, s) in [&d.primary, &d.home, &d.installed, &d.installed_home]
            .into_iter()
            .enumerate()
        {
            let b = encode_name(s, used).unwrap_or_else(|| {
                err = true;
                Vec::new()
            });
            r.bin4(e + 8 * j, i32::try_from(b.len()).unwrap_or(0));
            r.bin4(e + 8 * j + 4, i32::try_from(data).unwrap_or(0));
            r.bytes(data, &b);
            data += b.len();
        }
        r.bin4(e + 32, i32::try_from(d.obj_auts.len()).unwrap_or(0));
        r.bin4(
            e + 36,
            if d.obj_auts.is_empty() {
                0
            } else {
                i32::try_from(data).unwrap_or(0)
            },
        );
        for au in &d.obj_auts {
            r.char(data, 10, au);
            data += 10;
        }
        r.char(e + 40, 10, &d.data_aut);
        r.blank(e + 50, 2);
    }
    r.char(a + 16, 1, if err { "1" } else { "0" });
    r.blank(a + 17, 3);
}

/// QSZRTVPR.
pub fn qszrtvpr(
    store: &dyn ProductStore,
    sys: &SystemInfo,
    receiver_len: i32,
    format: &str,
    product_info: &[u8],
    info_format: Option<&str>,
) -> Result<Vec<u8>, ApiError> {
    const FORMATS: &[&str] = &[
        "PRDR0100", "PRDR0200", "PRDR0300", "PRDR0400", "PRDR0500", "PRDR0600", "PRDR0700",
        "PRDR0800",
    ];
    if !FORMATS.contains(&format) {
        return Err(ApiError::new("CPF3C21").text(format, 8));
    }
    if receiver_len < 8 {
        return Err(ApiError::new("CPF3C24"));
    }
    let pi = ProductInfo::read(product_info, info_format)?;
    let f = resolve(store, &pi)?;
    let mut r = Rcv::default();
    if format == "PRDR0700" {
        prdr0700(&f, &mut r);
    } else {
        base(&f, sys, &mut r, format != "PRDR0100");
        match format {
            "PRDR0200" => prdr0200(&f, &mut r),
            "PRDR0300" => prdr0300(&f, &mut r),
            "PRDR0400" => prdr0400(&f, &mut r),
            "PRDR0500" => prdr0500(&f, &mut r),
            "PRDR0600" => prdr0600(&f, sys, &mut r),
            "PRDR0800" => prdr0800(&f, pi.ccsid, &mut r),
            _ => {}
        }
    }
    r.deliver(receiver_len)
}
