// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file level: `DSPSIZ` first, since it names the size conditions
//! every later keyword may carry, then the rest.

use dds::diag::{Diagnostics, ids as dds_ids};
use dds::lex::{Keyword, Spec};

use super::{Cx, accept, classify, ind_text, name_parm, number_in, quoted};
use crate::cond::Entry;
use crate::diag::ids;
use crate::kw::Level;
use crate::model::{CmdKey, DisplaySize, FileAttrs, KeyName};

/// `CHGINPDFT` values.
const INPUT_DEFAULTS: &[&str] = &["BL", "CS", "HI", "RI", "UL", "ND", "FE", "LC", "ME", "MF"];

/// Apply the file-level specification.
pub(crate) fn apply(
    file: &mut FileAttrs,
    spec: &Spec,
    entry: Entry,
    cx: &Cx<'_>,
    diags: &mut Diagnostics,
) {
    for k in spec.keywords.iter().filter(|k| k.name == "DSPSIZ") {
        if classify(k, Level::File, diags).is_some() {
            dspsiz(file, k, diags);
        }
    }
    let cx = Cx::new(cx.source, cx.resolver, file);
    for k in &spec.keywords {
        if k.name == "DSPSIZ" {
            continue;
        }
        let Some(info) = classify(k, Level::File, diags) else {
            continue;
        };
        let used = accept(&cx, k, info, entry, diags);
        if let Some(key) = KeyName::parse(&k.name) {
            if let Some(ck) = cmd_key(k, key, &used.cond, diags) {
                add_key(&mut file.keys, ck, diags);
            }
        } else {
            keyword(file, k, diags);
        }
        file.keywords.push(used);
    }
}

/// A `CAnn`, `CFnn`, `ROLLUP`... keyword as a key. `PRINT(*PGM)` and
/// `PRINT(file)` have no indicator.
pub(crate) fn cmd_key(
    k: &Keyword,
    key: KeyName,
    cond: &crate::cond::Cond,
    diags: &mut Diagnostics,
) -> Option<CmdKey> {
    let first = k.parms.first().and_then(|p| p.as_word());
    let (ind, text) = if key == KeyName::Print
        && first.is_some_and(|w| w.starts_with('*') || w.parse::<u8>().is_err())
    {
        (None, None)
    } else {
        ind_text(k, 0, diags)?
    };
    if key == KeyName::VldCmdKey && ind.is_none() {
        diags.push(ids::response_indicator(k.line, &k.name, "is required"));
        return None;
    }
    Some(CmdKey {
        key,
        ind,
        text,
        cond: cond.clone(),
        line: k.line,
    })
}

/// Add a key to a level's list, refusing the same key twice.
pub(crate) fn add_key(keys: &mut Vec<CmdKey>, key: CmdKey, diags: &mut Diagnostics) {
    for k in keys.iter() {
        if k.key == key.key && k.cond == key.cond {
            diags.push(dds_ids::duplicate_keyword(key.line, &key.key.name()));
            return;
        }
        if let (Some(a), Some(b)) = (k.key.number(), key.key.number())
            && a == b
        {
            diags.push(ids::same_key_twice(
                key.line,
                &key.key.name(),
                &k.key.name(),
            ));
            return;
        }
    }
    keys.push(key);
}

fn dspsiz(file: &mut FileAttrs, k: &Keyword, diags: &mut Diagnostics) {
    let mut sizes: Vec<DisplaySize> = Vec::new();
    let words: Vec<&str> = k.parms.iter().filter_map(|p| p.as_word()).collect();
    if words.len() != k.parms.len() {
        diags.push(ids::bad_dspsiz(
            k.line,
            "parameters must be numbers or *DS3/*DS4",
        ));
        return;
    }
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let size = match w {
            "*DS3" => {
                i += 1;
                DisplaySize::ds3()
            }
            "*DS4" => {
                i += 1;
                DisplaySize::ds4()
            }
            _ => {
                let (Some(rows), Some(cols)) = (
                    w.parse::<u8>().ok(),
                    words.get(i + 1).and_then(|c| c.parse::<u8>().ok()),
                ) else {
                    diags.push(ids::bad_dspsiz(
                        k.line,
                        "a size is rows and columns, 24 80 or 27 132",
                    ));
                    return;
                };
                i += 2;
                let mut s = match (rows, cols) {
                    (24, 80) => DisplaySize::ds3(),
                    (27, 132) => DisplaySize::ds4(),
                    _ => {
                        diags.push(ids::bad_dspsiz(k.line, "sizes are 24 80 and 27 132"));
                        return;
                    }
                };
                if let Some(name) = words.get(i).filter(|n| n.starts_with('*')) {
                    if *name != s.name {
                        diags.push(ids::bad_dspsiz(
                            k.line,
                            &format!(
                                "{name} does not name the {} size; use {}",
                                s.label(),
                                s.name
                            ),
                        ));
                        return;
                    }
                    s.name = (*name).to_string();
                    i += 1;
                }
                s
            }
        };
        if sizes.iter().any(|s| s.rows == size.rows) {
            diags.push(ids::bad_dspsiz(k.line, "the same size is given twice"));
            return;
        }
        sizes.push(size);
    }
    if sizes.is_empty() || sizes.len() > 2 {
        diags.push(ids::bad_dspsiz(k.line, "one or two sizes are allowed"));
        return;
    }
    file.sizes = sizes;
}

fn keyword(file: &mut FileAttrs, k: &Keyword, diags: &mut Diagnostics) {
    match k.name.as_str() {
        "INDARA" => file.indara = true,
        "INDTXT" => indtxt(&mut file.indtxt, k, diags),
        "CHGINPDFT" => {
            if let Some(v) = chginpdft(k, diags) {
                file.chginpdft = Some(v);
            }
        }
        "REF" => {
            if let Some(mut r) = super::file_parm(k, 0, diags) {
                if k.parms.len() > 1
                    && let Some(fmt) = name_parm(k, 1, diags)
                    && let Ok(n) = qsname::Name::parse(&fmt)
                {
                    r = r.with_format(n);
                }
                file.ref_file = Some(r);
            }
        }
        "MSGLOC" => {
            if let Some(n) = number_in(k, 0, 1, 255, diags) {
                file.msgloc = Some(u8::try_from(n).unwrap_or(255));
            }
        }
        "RSTDSP" => file.rstdsp = true,
        "USRDSPMGT" => file.usrdspmgt = true,
        "CSRINPONLY" => file.csrinponly = true,
        "DSPRL" => file.dsprl = true,
        "ERRSFL" => file.errsfl = true,
        "ALTHELP" => file.alt_help = alt_key(k, KeyName::Ca(1), true, diags),
        "ALTPAGEUP" => file.alt_pageup = alt_key(k, KeyName::Cf(7), false, diags),
        "ALTPAGEDWN" => file.alt_pagedwn = alt_key(k, KeyName::Cf(8), false, diags),
        "CCSID" => {
            if let Some(n) = number_in(k, 0, 1, 65_535, diags) {
                file.ccsid = Some(u16::try_from(n).unwrap_or(u16::MAX));
            }
        }
        "MNUCNL" => {
            if let Some(ind) = mnucnl(k, diags) {
                file.responses.push((ind, k.line));
            }
        }
        _ => {}
    }
}

/// `INDTXT(indicator 'text')`.
pub(crate) fn indtxt(list: &mut Vec<(u8, String)>, k: &Keyword, diags: &mut Diagnostics) {
    let Some(n) = number_in(k, 0, 1, 99, diags) else {
        return;
    };
    if let Some(t) = quoted(k, 1, 50, diags) {
        list.push((u8::try_from(n).unwrap_or(99), t));
    }
}

/// `CHGINPDFT([attributes])`.
pub(crate) fn chginpdft(k: &Keyword, diags: &mut Diagnostics) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for p in &k.parms {
        match p.as_word() {
            Some(w) if INPUT_DEFAULTS.contains(&w) => out.push(w.to_string()),
            _ => {
                diags.push(ids::bad_value(
                    k.line,
                    &k.name,
                    &p.to_string(),
                    &INPUT_DEFAULTS.join(", "),
                ));
                return None;
            }
        }
    }
    Some(out)
}

/// `MNUCNL(CAnn [indicator])`: the indicator, when there is one.
pub(crate) fn mnucnl(k: &Keyword, diags: &mut Diagnostics) -> Option<u8> {
    let key = k
        .parms
        .first()
        .and_then(|p| p.as_word())
        .and_then(KeyName::parse);
    if !matches!(key, Some(KeyName::Ca(_) | KeyName::Cf(_))) {
        diags.push(ids::bad_value(
            k.line,
            &k.name,
            &k.parms[0].to_string(),
            "CAnn or CFnn",
        ));
        return None;
    }
    if k.parms.len() > 1 {
        return number_in(k, 1, 1, 99, diags).map(|n| u8::try_from(n).unwrap_or(99));
    }
    None
}

/// `ALTHELP([CAnn])`, `ALTPAGEUP([CFnn])`, `ALTPAGEDWN([CFnn])`.
fn alt_key(k: &Keyword, dft: KeyName, attention: bool, diags: &mut Diagnostics) -> Option<KeyName> {
    let Some(p) = k.parms.first() else {
        return Some(dft);
    };
    let key = p.as_word().and_then(KeyName::parse);
    match key {
        Some(KeyName::Ca(_)) if attention => key,
        Some(KeyName::Cf(_)) if !attention => key,
        _ => {
            diags.push(ids::bad_value(
                k.line,
                &k.name,
                &p.to_string(),
                if attention {
                    "CA01 to CA24"
                } else {
                    "CF01 to CF24"
                },
            ));
            None
        }
    }
}
