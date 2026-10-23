// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The editors: `EDTLIBL` (the user library list by sequence number),
//! `EDTOBJAUT` and `EDTAUTL` (authorities typed over, users added with
//! F6, the single authorities with F11) and `WRKSYSVAL`'s option 2.
//! Every change goes through the command that makes it (`CHGLIBL`,
//! `GRTOBJAUT`/`RVKOBJAUT`, `ADDAUTLE`/`CHGAUTLE`/`RMVAUTLE`,
//! `CHGSYSVAL`), so the checks and the audit are the commands'.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{cell, dashed, dialog, exec, parm, qual, service_text, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

/// The sequence numbers a page of `EDTLIBL` shows: three columns of 15.
const LIBL_PAGE: usize = 45;

/// `EDTLIBL` rows: the libraries ten apart from 10, the rest of the
/// numbers free, three to a row, a column of 15 a page.
fn libl_rows(libs: &[String]) -> Vec<Vec<(String, String)>> {
    let slots = (libs.len() + 1).div_ceil(LIBL_PAGE).max(1) * LIBL_PAGE;
    let name = |seq: usize| {
        (seq.is_multiple_of(10) && seq >= 10)
            .then(|| libs.get(seq / 10 - 1).cloned())
            .flatten()
            .unwrap_or_default()
    };
    let mut rows = Vec::new();
    for page in 0..slots / LIBL_PAGE {
        for i in 0..15 {
            let mut row = Vec::new();
            for col in 0..3 {
                let seq = (page * LIBL_PAGE + col * 15 + i) * 10;
                row.push((format!("SEQ{}", col + 1), seq.to_string()));
                row.push((format!("LIB{}", col + 1), name(seq)));
            }
            rows.push(row);
        }
    }
    rows
}

/// The libraries the rows hold, in sequence number order (a number
/// typed twice keeps the order on the display).
fn libl_of(d: &mut UimDialog) -> Result<Vec<String>, String> {
    let rows = d.list_mut("LIBS").rows.clone();
    let mut out: Vec<(u32, usize, String)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        for col in 1..=3 {
            let get = |v: String| {
                row.iter()
                    .find(|(k, _)| *k == v)
                    .map(|(_, x)| x.trim().to_ascii_uppercase())
                    .unwrap_or_default()
            };
            let lib = get(format!("LIB{col}"));
            if lib.is_empty() {
                continue;
            }
            let seq = get(format!("SEQ{col}"));
            let n: u32 = seq
                .parse()
                .map_err(|_| format!("Sequence number {seq} is not valid."))?;
            out.push((n, i * 3 + col, lib));
        }
    }
    out.sort();
    Ok(out.into_iter().map(|(_, _, l)| l).collect())
}

/// `EDTLIBL`: the user part of the library list; Enter applies it.
pub fn edtlibl(job: &mut Job, host: &Host, surface: &mut Surface) -> Option<Result<String, Fail>> {
    let mut d = match dialog(host, "QUIEDTLIBL") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let mut reload = true;
    let mut before = Vec::new();
    loop {
        if reload {
            before = host.attrs().libl.usr;
            super::fill(&mut d, "LIBS", libl_rows(&before));
            reload = false;
        }
        let resp = match show(&mut d, surface, "EDTLIBL") {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Some(Ok(String::new())),
            Response::Return(5) => reload = true,
            Response::Return(0) => {
                let after = match libl_of(&mut d) {
                    Ok(a) => a,
                    Err(m) => {
                        d.message = Some((m, true));
                        continue;
                    }
                };
                if after == before {
                    return Some(Ok(String::new()));
                }
                let list = if after.is_empty() {
                    "*NONE".to_string()
                } else {
                    after.join(" ")
                };
                match exec(job, host, &format!("CHGLIBL LIBL({list})")) {
                    Some((m, true)) => d.message = Some((m, true)),
                    _ => return Some(Ok(String::new())),
                }
            }
            _ => {}
        }
    }
}

/// The authority names in the order of the detail columns.
const BITS: [(&str, &str); 10] = [
    ("OPR", "*OBJOPR"),
    ("MGT", "*OBJMGT"),
    ("EXIST", "*OBJEXIST"),
    ("ALTER", "*OBJALTER"),
    ("REF", "*OBJREF"),
    ("READ", "*READ"),
    ("ADD", "*ADD"),
    ("UPD", "*UPD"),
    ("DLT", "*DLT"),
    ("EXE", "*EXECUTE"),
];

/// Where `DSPOBJAUT AUTFMT(*DETAIL)` puts each authority's `X`.
const BIT_COLS: [usize; 10] = [14, 18, 23, 29, 34, 39, 43, 47, 51, 55];

/// The value after `label` on the lines of a text display.
fn after(text: &str, label: &str) -> String {
    text.lines()
        .find_map(|l| {
            let at = l.find(label)?;
            let rest = l[at + label.len()..].split_once(':')?.1;
            Some(
                rest.split("   ")
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .unwrap_or_default()
}

/// `(user, group, authority)` rows of a `DSPOBJAUT` report.
fn auth_rows(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .skip_while(|l| !l.trim_start().starts_with("User"))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let cut = |a: usize, b: usize| {
                l.get(a..b.min(l.len()))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            (cut(1, 11), cut(13, 23), cut(25, l.len()))
        })
        .collect()
}

/// A user's single authorities from the detail report, `X` or blank in
/// `BITS` order.
fn bits_of(detail: &str, user: &str) -> Vec<String> {
    let line = detail
        .lines()
        .skip(1)
        .find(|l| l.get(1..11).is_some_and(|u| u.trim() == user))
        .unwrap_or("");
    BIT_COLS
        .iter()
        .map(|&c| {
            if line.get(c..=c) == Some("X") {
                "X".to_string()
            } else {
                String::new()
            }
        })
        .collect()
}

/// An entry of the editor: user, group, authority and its bits.
fn entry(user: &str, group: &str, aut: &str, bits: &[String]) -> Vec<(String, String)> {
    let mut row = vec![
        ("USER".to_string(), user.to_string()),
        ("GROUP".to_string(), group.to_string()),
        ("AUT".to_string(), aut.to_string()),
    ];
    for (i, (v, _)) in BITS.iter().enumerate() {
        row.push(((*v).to_string(), bits.get(i).cloned().unwrap_or_default()));
    }
    row
}

/// The single authorities an entry has X under, as `AUT()` values.
fn typed_bits(d: &UimDialog, list: &str, row: usize) -> Vec<&'static str> {
    BITS.iter()
        .filter(|(v, _)| !cell(d, list, row, v).is_empty())
        .map(|(_, n)| *n)
        .collect()
}

/// The view after F11: names, the object authorities, the data ones.
fn next_view(d: &mut UimDialog) {
    d.view = (d.view + 1) % 3;
    d.set(
        "AUTVIEW",
        match d.view {
            1 => "DTL",
            2 => "DTA",
            _ => "OBJ",
        },
    );
}

/// What a typed authority grants: a special value, or the single
/// authorities X'd when the detail columns show.
fn granted(d: &UimDialog, list: &str, row: usize, was: &str, was_bits: &[&str]) -> Option<String> {
    let aut = cell(d, list, row, "AUT").to_ascii_uppercase();
    if aut != was {
        return Some(aut);
    }
    if d.view == 0 {
        return None;
    }
    let bits = typed_bits(d, list, row);
    (bits != was_bits).then(|| {
        if bits.is_empty() {
            String::new()
        } else {
            bits.join(" ")
        }
    })
}

/// Users typed on the add panel with what to grant them.
fn new_users(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    grant: &dyn Fn(&str, &str) -> String,
) -> Result<(), Fail> {
    let view = d.view;
    d.view = usize::from(view == 2);
    let blank: Vec<Vec<(String, String)>> = (0..20).map(|_| entry("", "", "", &[])).collect();
    super::fill(d, "AUTS", blank);
    loop {
        match show(d, surface, "ADDUSERS")? {
            Response::Exit | Response::Cancel | Response::Gone => break,
            Response::Return(11) => {
                d.view = 1 - d.view.min(1);
                d.set("AUTVIEW", if d.view == 1 { "DTA" } else { "DTL" });
            }
            Response::Return(0) => {
                let n = d.list_mut("AUTS").rows.len();
                let mut done = 0;
                for i in 0..n {
                    let user = cell(d, "AUTS", i, "USER").to_ascii_uppercase();
                    if user.is_empty() {
                        continue;
                    }
                    let aut = cell(d, "AUTS", i, "AUT").to_ascii_uppercase();
                    let bits = typed_bits(d, "AUTS", i);
                    let aut = if aut.is_empty() { bits.join(" ") } else { aut };
                    let aut = if aut.is_empty() { "*USE".to_string() } else { aut };
                    d.message = exec(job, host, &grant(&user, &aut));
                    done += 1;
                }
                if done > 0 || d.message.is_none() {
                    break;
                }
            }
            _ => {}
        }
    }
    d.view = view;
    Ok(())
}

/// `EDTOBJAUT`: the object's authorities, typed over and applied with
/// `GRTOBJAUT` and `RVKOBJAUT`.
#[allow(clippy::too_many_lines)] // one arm per key
pub fn edtobjaut(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let (name, lib) = qual(r, "OBJ");
    let ty = parm(r, "OBJTYPE");
    let found = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), ty.clone()))?;
    let obj = format!(
        "OBJ({}/{}) OBJTYPE({})",
        found.lib, found.name, found.ty
    );
    let mut d = match dialog(host, "QUIEDTAUT") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("OBJ", &found.name);
    d.set("OBJLIB", &found.lib);
    d.set("OBJTYPE", &found.ty);
    d.set("OWNER", &found.owner);
    d.set("OBJHEAD", &dashed("Object", 27));
    d.set("DTAHEAD", &dashed("Data", 37));
    d.set("AUTVIEW", "OBJ");
    loop {
        let text = match service_text(host, &format!("DSPOBJAUT {obj}")) {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        };
        let detail =
            service_text(host, &format!("DSPOBJAUT {obj} AUTFMT(*DETAIL)")).unwrap_or_default();
        let pgp = after(&text, "Primary group");
        let autl = after(&text, "Authorization list");
        let autl = if autl.is_empty() { "*NONE".to_string() } else { autl };
        d.set("PGP", if pgp.is_empty() { "*NONE" } else { &pgp });
        d.set("AUTL", &autl);
        let orig: Vec<(String, String, Vec<String>)> = auth_rows(&text)
            .into_iter()
            .map(|(u, _, a)| {
                let b = bits_of(&detail, &u);
                (u, a, b)
            })
            .collect();
        let rows = auth_rows(&text)
            .iter()
            .zip(&orig)
            .map(|((u, g, a), (_, _, b))| entry(u, g, a, b))
            .collect();
        super::fill(&mut d, "AUTS", rows);
        'show: loop {
            let resp = match show(&mut d, surface, "EDTOBJAUT") {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            match resp {
                Response::Exit | Response::Cancel | Response::Gone => {
                    return Some(Ok(String::new()));
                }
                Response::Return(5) => break 'show,
                Response::Return(11) => next_view(&mut d),
                Response::Return(10) => {
                    d.message = exec(job, host, &format!("?GRTOBJAUT {obj} REFOBJ()"));
                    break 'show;
                }
                Response::Return(6) => {
                    let grant = |u: &str, a: &str| format!("GRTOBJAUT {obj} USER({u}) AUT({a})");
                    if let Err(e) = new_users(job, host, surface, &mut d, &grant) {
                        return Some(Err(e));
                    }
                    break 'show;
                }
                Response::Return(0) => {
                    let mut changed = false;
                    for (i, (user, was, was_bits)) in orig.iter().enumerate() {
                        let was_names: Vec<&str> = BITS
                            .iter()
                            .zip(was_bits)
                            .filter(|(_, x)| !x.is_empty())
                            .map(|((_, n), _)| *n)
                            .collect();
                        let Some(aut) = granted(&d, "AUTS", i, was, &was_names) else {
                            continue;
                        };
                        changed = true;
                        let line = if aut.is_empty() && user == "*PUBLIC" {
                            format!("GRTOBJAUT {obj} USER(*PUBLIC) AUT(*EXCLUDE) REPLACE(*YES)")
                        } else if aut.is_empty() {
                            format!("RVKOBJAUT {obj} USER({user}) AUT(*ALL)")
                        } else {
                            format!("GRTOBJAUT {obj} USER({user}) AUT({aut}) REPLACE(*YES)")
                        };
                        if let Some(m) = exec(job, host, &line) {
                            d.message = Some(m);
                        }
                    }
                    let new = d.get("AUTL").trim().to_ascii_uppercase();
                    if new != autl {
                        changed = true;
                        let line = if new.is_empty() || new == "*NONE" {
                            format!("RVKOBJAUT {obj} AUTL({autl})")
                        } else {
                            format!("GRTOBJAUT {obj} AUTL({new})")
                        };
                        if let Some(m) = exec(job, host, &line) {
                            d.message = Some(m);
                        }
                    }
                    if !changed {
                        return Some(Ok(String::new()));
                    }
                    break 'show;
                }
                Response::Options(c) => clear_options(&mut d, &c),
                _ => {}
            }
        }
    }
}

fn clear_options(d: &mut UimDialog, c: &[Choice]) {
    for ch in c {
        d.list_mut(&ch.list).options.clear();
    }
}

/// `EDTAUTL`: the list's users, typed over and applied with
/// `CHGAUTLE`, `RMVAUTLE` and `ADDAUTLE`.
pub fn edtautl(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let name = parm(r, "AUTL");
    let mut d = match dialog(host, "QUIEDTAUT") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("OBJ", &name);
    d.set("OBJLIB", "QSYS");
    d.set("OBJTYPE", "*AUTL");
    d.set("OBJHEAD", &dashed("Object", 27));
    d.set("DTAHEAD", &dashed("Data", 37));
    d.set("AUTVIEW", "OBJ");
    loop {
        let rows = host
            .link
            .borrow_mut()
            .with(|l| l.autl_panel(name.clone()))
            .ok()?;
        let owner = rows.first().and_then(|r| r.first()).cloned().unwrap_or_default();
        d.set("OWNER", &owner);
        let orig: Vec<(String, String, bool)> = rows
            .iter()
            .skip(1)
            .map(|r| {
                (
                    r.first().cloned().unwrap_or_default(),
                    r.get(1).cloned().unwrap_or_default(),
                    r.get(2).is_some_and(|x| x.trim() == "X"),
                )
            })
            .collect();
        let entries = orig
            .iter()
            .map(|(u, a, m)| {
                let mut e = entry(u, "", a, &[]);
                e.push(("LSTMGT".into(), if *m { "X" } else { "" }.into()));
                e
            })
            .collect();
        super::fill(&mut d, "AUTS", entries);
        'show: loop {
            let resp = match show(&mut d, surface, "EDTAUTL") {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            match resp {
                Response::Exit | Response::Cancel | Response::Gone => {
                    return Some(Ok(String::new()));
                }
                Response::Return(5) => break 'show,
                Response::Return(11) => next_view(&mut d),
                Response::Return(6) => {
                    let grant =
                        |u: &str, a: &str| format!("ADDAUTLE AUTL({name}) USER({u}) AUT({a})");
                    if let Err(e) = new_users(job, host, surface, &mut d, &grant) {
                        return Some(Err(e));
                    }
                    break 'show;
                }
                Response::Return(0) => {
                    let mut changed = false;
                    for (i, (user, was, mgt)) in orig.iter().enumerate() {
                        let now_mgt = !cell(&d, "AUTS", i, "LSTMGT").is_empty();
                        let aut = granted(&d, "AUTS", i, was, &[]);
                        if aut.is_none() && now_mgt == *mgt {
                            continue;
                        }
                        changed = true;
                        let aut = aut.unwrap_or_else(|| was.clone());
                        let line = if aut.is_empty() && user != "*PUBLIC" {
                            format!("RMVAUTLE AUTL({name}) USER({user})")
                        } else {
                            let aut = if aut.is_empty() { "*EXCLUDE".into() } else { aut };
                            let extra = if now_mgt { " *AUTLMGT" } else { "" };
                            format!("CHGAUTLE AUTL({name}) USER({user}) AUT({aut}{extra})")
                        };
                        if let Some(m) = exec(job, host, &line) {
                            d.message = Some(m);
                        }
                    }
                    if !changed {
                        return Some(Ok(String::new()));
                    }
                    break 'show;
                }
                _ => {}
            }
        }
    }
}

/// Whether a system value is a list: shown one element to a row.
fn is_list(name: &str, elems: &[String]) -> bool {
    elems.len() > 1 || name.ends_with("LIBL") || matches!(name, "QAUDLVL" | "QAUDLVL2" | "QSETJOBATR")
}

/// `WRKSYSVAL` option 2: each system value typed 2 against, changed on
/// the change panel; the other options are left for the caller.
pub fn change_options(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &UimDialog,
    answer: Response,
) -> Response {
    let Response::Options(choices) = answer else {
        return answer;
    };
    let (twos, rest): (Vec<Choice>, Vec<Choice>) =
        choices.into_iter().partition(|c| c.option.trim() == "2");
    for c in &twos {
        let name = cell(d, &c.list, c.row, "SVNAME");
        let _ = chgsysval(job, host, surface, &name);
    }
    if rest.is_empty() {
        Response::Return(5)
    } else {
        Response::Options(rest)
    }
}

/// The change panel of one system value.
fn chgsysval(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    name: &str,
) -> Option<Result<String, Fail>> {
    let mut d = match dialog(host, "QUICHGSV") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    loop {
        let (text, group, elems) = host
            .link
            .borrow_mut()
            .with(|l| l.sysval_panel(name.to_string()))?;
        let list = is_list(name, &elems);
        d.set("SVNAME", name);
        d.set("SVTEXT", &text);
        d.set("SVTYPE", &group);
        let panel = if list {
            let mut rows: Vec<Vec<(String, String)>> = elems
                .iter()
                .filter(|e| !e.trim().is_empty() && e.trim() != "*NONE")
                .map(|e| vec![("SVELEM".to_string(), e.trim().to_string())])
                .collect();
            while rows.len() < 30 {
                rows.push(vec![("SVELEM".to_string(), String::new())]);
            }
            super::fill(&mut d, "ELEMS", rows);
            "CHGSVL"
        } else {
            d.set("SVVALUE", elems.first().map_or("", |e| e.trim()));
            "CHGSV"
        };
        let before = elems.join(" ");
        let resp = match show(&mut d, surface, panel) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Some(Ok(String::new())),
            Response::Return(0) => {
                let value = if list {
                    let n = d.list_mut("ELEMS").rows.len();
                    let v: Vec<String> = (0..n)
                        .map(|i| cell(&d, "ELEMS", i, "SVELEM"))
                        .filter(|e| !e.is_empty())
                        .collect();
                    if v.is_empty() { "*NONE".to_string() } else { v.join(" ") }
                } else {
                    let v = d.get("SVVALUE").trim().to_string();
                    let quoted = !v.starts_with('*') && v.contains(|c: char| !c.is_ascii_digit());
                    if quoted { format!("'{}'", v.replace('\'', "''")) } else { v }
                };
                if value.trim_matches('\'') == before.trim() {
                    return Some(Ok(String::new()));
                }
                match exec(job, host, &format!("CHGSYSVAL SYSVAL({name}) VALUE({value})")) {
                    Some((m, true)) => d.message = Some((m, true)),
                    _ => return Some(Ok(String::new())),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libl_rows_number_the_libraries() {
        let rows = libl_rows(&["QGPL".to_string(), "QTEMP".to_string()]);
        assert_eq!(rows.len(), 15);
        assert_eq!(rows[0][0].1, "0");
        assert_eq!(rows[1][0].1, "10");
        assert_eq!(rows[1][1].1, "QGPL");
        assert_eq!(rows[2][1].1, "QTEMP");
        assert_eq!(rows[0][2].1, "150");
        assert_eq!(rows[14][4].1, "440");
    }

    #[test]
    fn detail_bits_by_column() {
        let line = |u: &str, xs: &[usize]| {
            let mut l: Vec<char> = format!(" {u:<10}").chars().collect();
            l.resize(56, ' ');
            for &c in xs {
                l[c] = 'X';
            }
            l.into_iter().collect::<String>()
        };
        let detail = format!(
            " User
{}
{}
",
            line("QPGMR", &BIT_COLS),
            line("*PUBLIC", &[14, 39, 55])
        );
        assert!(bits_of(&detail, "QPGMR").iter().all(|b| b == "X"));
        let p = bits_of(&detail, "*PUBLIC");
        assert_eq!(p[0], "X");
        assert_eq!(p[1], "");
        assert_eq!(p[5], "X");
        assert_eq!(p[9], "X");
    }

    #[test]
    fn report_rows_and_labels() {
        let text = " Object . . . . . . . :   X            Owner  . . . . . . . :   QPGMR\n   Library  . . . . . :   QGPL         Primary group  . . . :   *NONE\n Object type  . . . . :   *JOBD        Authorization list . :   *NONE\n                          Object\n User        Group       Authority\n QPGMR                   *ALL\n *PUBLIC                 *USE\n";
        assert_eq!(after(text, "Primary group"), "*NONE");
        let rows = auth_rows(text);
        assert_eq!(rows[0], ("QPGMR".into(), String::new(), "*ALL".into()));
        assert_eq!(rows[1].2, "*USE");
    }
}
