// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Control and file description specifications. The overflow
//! indicator, key location, file condition, `SPECIAL` routine, line
//! counter specification and the continuation lines (`K` in column 53)
//! become ILE RPG keywords.

use super::place::{Rec, cut3};
use super::Out;

/// The line counter specifications: form length and overflow line by file.
#[derive(Default)]
pub(super) struct Files {
    forms: Vec<(String, String, String)>,
}

/// Whether an F record is a continuation line.
pub(super) fn is_continuation(r: &str) -> bool {
    cut3("FK", r, "cont") == "K" && cut3("F", r, "name").is_empty()
}

/// The control specification's keywords.
pub(super) fn control(r: &str) -> String {
    let mut kws = Vec::new();
    if cut3("H", r, "debug") == "1" {
        kws.push("DEBUG".to_string());
    }
    let cur = rpgspecs::rpg3_form("H").cut(r, "currency");
    if !cur.trim().is_empty() {
        kws.push(format!("CURSYM('{cur}')"));
    }
    match cut3("H", r, "datfmt").as_str() {
        "M" => kws.push("DATEDIT(*MDY)".into()),
        "D" => kws.push("DATEDIT(*DMY)".into()),
        "Y" => kws.push("DATEDIT(*YMD)".into()),
        _ => {}
    }
    match cut3("H", r, "decedit").as_str() {
        "I" => kws.push("DECEDIT(',')".into()),
        "J" => kws.push("DECEDIT('0,')".into()),
        "D" => kws.push("DECEDIT('0.')".into()),
        _ => {}
    }
    if cut3("H", r, "altseq") == "S" {
        kws.push("ALTSEQ(*SRC)".into());
    }
    if cut3("H", r, "ftrans") == "F" {
        kws.push("FTRANS".into());
    }
    let mut h = Rec::new("H", r);
    h.put("keywords", &kws.join(" "));
    h.text()
}

impl Files {
    /// Note a line counter specification.
    pub(super) fn line_counter(&mut self, r: &str) {
        self.forms.push((
            cut3("L", r, "name"),
            cut3("L", r, "formlen"),
            cut3("L", r, "overflow"),
        ));
    }

    /// A file description and its continuation lines: the converted
    /// records with the records they came from.
    pub(super) fn convert(&self, recs: &[String], at: usize, out: &mut Out) -> Vec<(String, usize)> {
        let r = &recs[0];
        let name = cut3("F", r, "name");
        let mut f = Rec::new("F", r);
        f.put("name", &name)
            .put("type", &cut3("F", r, "type"))
            .put("designation", &cut3("F", r, "designation"))
            .put("eof", &cut3("F", r, "eof"))
            .put("addition", &cut3("F", r, "addition"))
            .put("sequence", &cut3("F", r, "sequence"))
            .put("format", &cut3("F", r, "format"))
            .right("reclen", &cut3("F", r, "reclen"))
            .put("limits", &cut3("F", r, "limits"))
            .right("keylen", &cut3("F", r, "keylen"))
            .put("rectype", &cut3("F", r, "rectype"))
            .put("org", &cut3("F", r, "org"))
            .put("device", &cut3("F", r, "device"));
        let mut kws: Vec<String> = Vec::new();
        let ofl = cut3("F", r, "overflow");
        if !ofl.is_empty() {
            kws.push(format!("OFLIND(*IN{ofl})"));
        }
        let kloc = cut3("F", r, "keystart");
        if !kloc.is_empty() {
            kws.push(format!("KEYLOC({kloc})"));
        }
        let routine = cut3("F", r, "routine");
        if cut3("F", r, "device") == "SPECIAL" && !routine.is_empty() {
            kws.push(format!("PGMNAME('{routine}')"));
            let entry = cut3("F", r, "entry");
            if !entry.is_empty() {
                kws.push(format!("PLIST({entry})"));
            }
        }
        if let Some((_, len, of)) = self.forms.iter().find(|(n, _, _)| *n == name) {
            if !len.is_empty() {
                kws.push(format!("FORMLEN({len})"));
            }
            if !of.is_empty() {
                kws.push(format!("FORMOFL({of})"));
            }
        }
        let cond = cut3("F", r, "condition");
        if !cond.is_empty() {
            kws.push(format!("EXTIND(*IN{cond})"));
        }
        let mut lines = Vec::new();
        let mut from = at;
        for (k, c) in recs.iter().enumerate().skip(1) {
            let entry = cut3("FK", c, "entry");
            let ext = cut3("FK", c, "extname");
            let kw = match cut3("FK", c, "option").as_str() {
                "RENAME" => format!("RENAME({ext}:{entry})"),
                "IGNORE" => format!("IGNORE({ext})"),
                "INCLUD" => format!("INCLUDE({ext})"),
                "INFDS" => format!("INFDS({entry})"),
                "INFSR" => format!("INFSR({entry})"),
                "SFILE" => format!("SFILE({ext}:{})", cut3("FK", c, "rrn")),
                "COMIT" => "COMMIT".to_string(),
                "PLIST" => format!("PLIST({entry})"),
                "ID" => format!("DEVID({entry})"),
                "NUM" => format!("MAXDEV(*FILE)"),
                "SLN" => format!("SLN({entry})"),
                "PRTCTL" => format!("PRTCTL({entry})"),
                "FMTS" => format!("FMTS('{entry}')"),
                "SAVEDS" => format!("SAVEDS({entry})"),
                "SAVEIN" => "SAVEIND(99)".to_string(),
                _ => {
                    out.msg("RNM0507", 30, at + k);
                    continue;
                }
            };
            // The first keyword that does not fit goes on a line of its own.
            lines.push((kw, at + k));
            from = at + k;
        }
        let _ = from;
        let mut result = Vec::new();
        let main = kws.join(" ");
        f.put("keywords", &main);
        result.push((f.text(), at));
        for (kw, k) in lines {
            let mut c = Rec::new("F", &recs[k - at]);
            c.put("keywords", &kw);
            result.push((c.text(), k));
        }
        result
    }
}
