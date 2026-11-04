// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Input specifications. Record and field lines of files stay input
//! specifications at the ILE RPG columns; data structures, their
//! subfields and named constants become definitions.

use super::defs::Defs;
use super::place::{Rec, cut3, raw3};
use super::{Out, Sect};

/// Where the input specifications are: in a file's records or in a
/// data structure.
#[derive(Default)]
pub(super) struct Input {
    in_ds: bool,
    /// The structure is externally described (subfield lines rename).
    ext_ds: bool,
    /// The last record line was externally described.
    ext_rec: bool,
}

/// The ILE RPG data type of an RPG III subfield's format.
fn data_type(fmt: &str) -> &'static str {
    match fmt {
        "P" => "P",
        "B" => "B",
        _ => "",
    }
}

impl Input {
    /// Convert one input specification.
    pub(super) fn convert(&mut self, r: &str, at: usize, out: &mut Out, _defs: &mut Defs) {
        if cut3("IDS", r, "ds") == "DS" {
            self.in_ds = true;
            self.ext_ds = cut3("IDS", r, "ext") == "E";
            out.push(Sect::D, ds_line(r), at);
            return;
        }
        let rec_name = cut3("I", r, "name");
        let rel = cut3("I", r, "relation");
        let record_line = !rec_name.is_empty() || rel == "AND" || rel == "OR";
        if record_line {
            self.in_ds = false;
            // An externally described record has no sequence.
            self.ext_rec = cut3("I", r, "sequence").is_empty() && rel.is_empty();
            out.push(Sect::I, record(r, self.ext_rec), at);
            return;
        }
        if self.in_ds {
            if cut3("IX", r, "format") == "C" {
                out.push(Sect::D, constant(r), at);
            } else {
                out.push(Sect::D, subfield(r, self.ext_ds), at);
            }
            return;
        }
        if cut3("IX", r, "format") == "C" {
            out.push(Sect::D, constant(r), at);
            return;
        }
        if self.ext_rec {
            out.push(Sect::I, ext_field(r), at);
        } else {
            out.push(Sect::I, field(r), at);
        }
    }
}

/// A record line: program described with its identification codes, or
/// an externally described format with its indicator.
fn record(r: &str, ext: bool) -> String {
    let mut i = Rec::new("I", r);
    let rel = cut3("I", r, "relation");
    if rel == "AND" || rel == "OR" {
        i.put("relation", &rel);
    } else {
        i.put("name", &cut3("I", r, "name"));
        if !ext {
            i.put("sequence", &cut3("I", r, "sequence"))
                .put("number", &cut3("I", r, "number"))
                .put("option", &cut3("I", r, "option"));
        }
    }
    i.put("rid", &cut3("I", r, "rid"));
    for k in 1..=3 {
        i.right(&format!("pos{k}"), &cut3("I", r, &format!("pos{k}")))
            .put(&format!("not{k}"), &cut3("I", r, &format!("not{k}")))
            .put(&format!("part{k}"), &cut3("I", r, &format!("part{k}")))
            .put(&format!("char{k}"), &raw3("I", r, &format!("char{k}")).trim_end().to_string());
    }
    i.text()
}

/// A program-described field line.
fn field(r: &str) -> String {
    let mut i = Rec::new("IX", r);
    i.put("format", &cut3("IX", r, "format"))
        .right("from", &cut3("IX", r, "from"))
        .right("to", &cut3("IX", r, "to"))
        .right("decimals", &cut3("IX", r, "decimals"))
        .put("field", &cut3("IX", r, "field"))
        .put("level", &cut3("IX", r, "level"))
        .put("match", &cut3("IX", r, "match"))
        .put("relation", &cut3("IX", r, "relation"))
        .put("plus", &cut3("IX", r, "plus"))
        .put("minus", &cut3("IX", r, "minus"))
        .put("zero", &cut3("IX", r, "zero"));
    i.text()
}

/// An externally described field line.
fn ext_field(r: &str) -> String {
    let mut i = Rec::new("IF", r);
    i.put("extname", &cut3("IF", r, "extname"))
        .put("field", &cut3("IF", r, "field"))
        .put("level", &cut3("IF", r, "level"))
        .put("match", &cut3("IF", r, "match"))
        .put("plus", &cut3("IF", r, "plus"))
        .put("minus", &cut3("IF", r, "minus"))
        .put("zero", &cut3("IF", r, "zero"));
    i.text()
}

/// A data structure line.
fn ds_line(r: &str) -> String {
    let mut d = Rec::new("D", r);
    let name = cut3("IDS", r, "name");
    let mut kws = Vec::new();
    let ext = cut3("IDS", r, "ext") == "E";
    if ext {
        d.put("ext", "E");
        kws.push(format!("EXTNAME({})", cut3("IDS", r, "extfile")));
    }
    match cut3("IDS", r, "option").as_str() {
        "S" => {
            d.put("dstype", "S");
        }
        "U" => {
            d.put("dstype", "U");
        }
        "I" => kws.push("INZ".into()),
        _ => {}
    }
    let occurs = cut3("IDS", r, "occurs");
    if !occurs.is_empty() {
        kws.push(format!("OCCURS({occurs})"));
    }
    d.put("name", &if name.is_empty() { String::new() } else { format!(" {name}") })
        .put("deftype", "DS")
        .right("to", &cut3("IDS", r, "len"))
        .put("keywords", &kws.join(" "));
    d.text()
}

/// A subfield: positions or a special keyword, the initial value as
/// `INZ`; in an externally described structure a rename (`EXTFLD`).
fn subfield(r: &str, ext: bool) -> String {
    let mut d = Rec::new("D", r);
    let name = cut3("IX", r, "field");
    d.put("name", &format!("  {name}"));
    if ext {
        let extname = cut3("IF", r, "extname");
        d.put("ext", "E").put("keywords", &format!("EXTFLD({extname})"));
        return d.text();
    }
    let kw = cut3("IX", r, "keyword");
    if kw.starts_with('*') {
        d.put("from", &kw);
    } else {
        d.right("from", &cut3("IX", r, "from")).right("to", &cut3("IX", r, "to"));
    }
    d.put("type", data_type(&cut3("IX", r, "format"))).right("decimals", &cut3("IX", r, "decimals"));
    if cut3("IX", r, "init") == "I" {
        let v = cut3("IX", r, "value");
        d.put("keywords", &if v.is_empty() { "INZ".to_string() } else { format!("INZ({v})") });
    }
    d.text()
}

/// A named constant.
fn constant(r: &str) -> String {
    let mut d = Rec::new("D", r);
    d.put("name", &format!(" {}", cut3("IX", r, "field")))
        .put("deftype", "C")
        .put("keywords", &format!("CONST({})", cut3("IX", r, "value")));
    d.text()
}
