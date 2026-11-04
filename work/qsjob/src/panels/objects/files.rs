// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPFD`, `DSPFFD`, `DSPDBR` as their printed form in a spooled file
//! view (`QUIDSPFD`), `DSPDTAARA` (`QUIDSPDTAA`) and `WRKOBJOWN`
//! (`QUIWRKOOWN`).

use cmd::Resolved;
use mi::Job;
use mi::io::DataAreaKind;
use stg::Client;

use super::{act, common, entry, fill, parm, qual, show};
use crate::command::Fail;
use crate::command::jobsite::leading_msgid;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;
use crate::panels::dialog;

/// Lines a page of the spooled file view.
const SPLF_ROWS: usize = 19;

/// The command's text form, run in the storage service.
fn text_form(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let word = sys_cpps::entry(&r.name).map_or_else(|| r.name.to_ascii_lowercase(), |e| e.word.clone());
    let line = sys_cpps::bridge::specified_line(&word, &crate::command::parms_of(r));
    let text = host.exec(&line).text;
    let first = text.lines().next().unwrap_or("").trim();
    if let Some(id) = leading_msgid(first) {
        return Err(Fail::new(id, text.trim_end().to_string()));
    }
    Ok(text)
}

/// `DSPFD`, `DSPFFD`, `DSPDBR`: the printed description in a spooled
/// file view, with Control (`T`, `B`, `+n`, `-n`) and Find.
pub fn splf(host: &Host, surface: &mut Surface, r: &Resolved, file: &str) -> Result<String, Fail> {
    let text = text_form(host, r)?;
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.chars().take(78).collect())
        .collect();
    let n = lines.len();
    let mut d = dialog(host, "QUIDSPFD")?;
    d.set("SPLF", file);
    fill(&mut d, "LINES", lines.iter().map(|l| entry(&[("LINE", l.clone())])).collect());
    let mut find = String::new();
    loop {
        let top = d.list_mut("LINES").top;
        d.set("PAGELINE", &format!("1/{}", top + 1));
        let resp = show(&mut d, surface, "SPLF")?;
        let control = d.get("CONTROL").trim().to_ascii_uppercase();
        d.set("CONTROL", "");
        let typed = d.get("FIND").trim().to_string();
        if !typed.is_empty() {
            find = typed;
        }
        let mut search = matches!(resp, Response::Return(16)) || !d.get("FIND").trim().is_empty();
        if matches!(resp, Response::Exit | Response::Cancel | Response::Gone) {
            return Ok(String::new());
        }
        let last = n.saturating_sub(SPLF_ROWS);
        let l = d.list_mut("LINES");
        match control.as_str() {
            "" => {}
            "T" | "*TOP" => l.top = 0,
            "B" | "*BOT" => l.top = last,
            c => {
                if let Ok(k) = c.parse::<i64>() {
                    let to = i64::try_from(l.top).unwrap_or(0) + k;
                    l.top = usize::try_from(to.max(0)).unwrap_or(0).min(n.saturating_sub(1));
                } else {
                    d.error("CPD9B95", &[c]);
                    search = false;
                }
            }
        }
        if search && !find.is_empty() {
            d.set("FIND", "");
            let from = d.list_mut("LINES").top + 1;
            let hit = (from..n)
                .chain(0..from)
                .find(|i| lines[*i].to_ascii_uppercase().contains(&find.to_ascii_uppercase()));
            match hit {
                Some(i) => d.list_mut("LINES").top = i,
                None => d.error("CPD9B8C", &[&find]),
            }
        }
    }
}

/// `DSPDTAARA`: a data area's attributes and value.
pub fn dspdtaara(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (name, lib) = qual(r, "DTAARA", "*LIBL");
    let found = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), "*DTAARA".into()))
        .ok_or_else(|| Fail::new("CPF1015", format!("Data area {name} in {lib} not found.")))?;
    let value = host
        .link
        .borrow_mut()
        .with(|l| l.da_read(found.name.clone(), found.lib.clone()))
        .map_err(|e| crate::panels::failed(format!("{e:?}")))?;
    let mut d = dialog(host, "QUIDSPDTAA")?;
    d.set("DTAARA", &found.name);
    d.set("DTALIB", &found.lib);
    d.set("DTATEXT", &found.text);
    let (ty, len, rows) = match value.kind {
        DataAreaKind::Dec => {
            let shown = u8::try_from(value.len)
                .ok()
                .zip(u8::try_from(value.decimals).ok())
                .and_then(|(p, s)| qdecimal::Packed::new(p, s).ok())
                .and_then(|p| p.decode(&value.value).ok())
                .map(|v| v.to_string())
                .unwrap_or_default();
            ("*DEC", format!("{} {}", value.len, value.decimals), vec![shown])
        }
        DataAreaKind::Lgl => ("*LGL", "1".to_string(), vec![ccsid::from_ebcdic(&value.value)]),
        DataAreaKind::Char => {
            // In quotes, as the system shows a character value.
            let text: Vec<char> = ccsid::from_ebcdic(&value.value).chars().collect();
            let mut rows: Vec<String> = text.chunks(50).map(|c| c.iter().collect()).collect();
            if rows.is_empty() {
                rows.push(String::new());
            }
            let last = rows.len() - 1;
            rows[0].insert(0, '\'');
            rows[last].push('\'');
            ("*CHAR", value.len.to_string(), rows)
        }
    };
    d.set("DTATYPE", ty);
    d.set("DTALEN", &len);
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(i, v): (usize, String)| entry(&[("OFFSET", (i * 50).to_string()), ("VALUE", v)]))
        .collect();
    fill(&mut d, "VALUES", rows);
    loop {
        match show(&mut d, surface, "DSPDTAARA")? {
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => {
                return Ok(String::new());
            }
            _ => {}
        }
    }
}

/// `WRKOBJOWN`: the objects a user profile owns, with their options.
pub fn wrkobjown(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let user = parm(r, "USRPRF")
        .filter(|u| !u.is_empty() && u != "*CURRENT")
        .unwrap_or_else(|| super::user(host));
    let mut d = dialog(host, "QUIWRKOOWN")?;
    d.set("USRPRF", &user);
    let mut last = String::new();
    loop {
        let data = crate::panels::panel_data(host, "WRKOBJOWN", "OBJOWN", &[&user])?;
        d.set("USRPRF", &crate::panels::data_var(&data, "USRPRF"));
        fill(&mut d, "OBJS", crate::panels::data_list(&data, "OBJS"));
        let resp = show(&mut d, surface, "WRKOBJOWN")?;
        let Some(resp) = common(job, host, &mut d, resp, &mut last) else {
            continue;
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(11) => {
                d.view = 1 - d.view.min(1);
                d.set("OOVIEW", if d.view == 1 { "2" } else { "1" });
            }
            Response::Return(22) => d.info("CPI9B81", &[]),
            Response::Options(c) => act(job, host, &mut d, &c, &|d, c| {
                (c.option == "4").then(|| {
                    let row = d.row(&c.list, c.row);
                    let get = |k: &str| {
                        row.iter()
                            .find(|(v, _)| v == k)
                            .map(|(_, x)| x.clone())
                            .unwrap_or_default()
                    };
                    let (n, l, t) = (get("OBJ"), get("OBJLIB"), get("OBJTYPE"));
                    super::libs::delete_command(&n, &l, &t)
                        .unwrap_or_else(|| format!("?DLTOBJ OBJ({l}/{n}) OBJTYPE({t})"))
                })
            }),
            _ => {}
        }
    }
}
