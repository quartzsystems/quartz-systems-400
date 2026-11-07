// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extension specifications as definitions: arrays and tables with
//! `DIM`, `CTDATA` or `FROMFILE`, `TOFILE`, `PERRCD`, the sequence and
//! an alternating array as a definition of its own with `ALT`.

use super::Out;
use super::place::{Rec, cut3};

/// What the definitions know: the arrays the extension specifications
/// declared.
#[derive(Default)]
pub(super) struct Defs {
    /// Array and table names, in order.
    pub(super) arrays: Vec<String>,
}

/// A standalone definition line.
fn standalone(from: &str, name: &str, len: &str, typ: &str, dec: &str, kws: &[String]) -> String {
    let mut d = Rec::new("D", from);
    d.put("name", &format!(" {name}"))
        .put("deftype", "S")
        .right("to", len)
        .put("type", typ)
        .right("decimals", dec)
        .put("keywords", &kws.join(" "));
    d.text()
}

/// The ILE RPG data type of an RPG III data format.
fn data_type(fmt: &str) -> &'static str {
    match fmt {
        "P" => "P",
        "B" => "B",
        _ => "",
    }
}

impl Defs {
    /// An extension specification: its definitions.
    pub(super) fn extension(&mut self, r: &str, at: usize, out: &mut Out) -> Vec<String> {
        let name = cut3("E", r, "name");
        if name.is_empty() {
            // A record address file or a file translation entry.
            return Vec::new();
        }
        let entries = cut3("E", r, "entries");
        let per = cut3("E", r, "perrcd");
        let from = cut3("E", r, "from");
        let to = cut3("E", r, "to");
        let fmt = cut3("E", r, "format");
        if fmt == "L" || fmt == "R" {
            out.msg("RNM0591", 30, at);
        }
        let mut kws = vec![format!("DIM({entries})")];
        if !from.is_empty() {
            kws.push(format!("FROMFILE({from})"));
        } else if !per.is_empty() {
            kws.push("CTDATA".into());
        }
        if !to.is_empty() {
            kws.push(format!("TOFILE({to})"));
        }
        if !per.is_empty() {
            kws.push(format!("PERRCD({per})"));
        }
        match cut3("E", r, "seq").as_str() {
            "A" => kws.push("ASCEND".into()),
            "D" => kws.push("DESCEND".into()),
            _ => {}
        }
        self.arrays.push(name.clone());
        let mut lines = vec![standalone(
            r,
            &name,
            &cut3("E", r, "len"),
            data_type(&fmt),
            &cut3("E", r, "decimals"),
            &kws,
        )];
        let alt = cut3("E", r, "alt");
        if !alt.is_empty() {
            let mut akws = vec![format!("DIM({entries})"), format!("ALT({name})")];
            match cut3("E", r, "altseq").as_str() {
                "A" => akws.push("ASCEND".into()),
                "D" => akws.push("DESCEND".into()),
                _ => {}
            }
            self.arrays.push(alt.clone());
            lines.push(standalone(
                r,
                &alt,
                &cut3("E", r, "altlen"),
                data_type(&cut3("E", r, "altformat")),
                &cut3("E", r, "altdecimals"),
                &akws,
            ));
        }
        lines
    }
}
