// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help specifications (`H`): each `HLPARA` names a part of the record
//! (`*RCD`, `*FLD name`, `*CNST n` or a box of rows and columns) and the
//! `HLPPNLGRP` after it the UIM help module that explains it. The job
//! shows the module under the cursor when Help is pressed and the
//! record does not return Help to the program.

use crate::model::{KeywordUse, Record};

/// What a help specification covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelpArea {
    /// `HLPARA(*RCD)`: the whole record.
    Record,
    /// `HLPARA(*FLD name)`.
    Field(String),
    /// `HLPARA(*CNST n)`: the record's n-th constant.
    Constant(u32),
    /// `HLPARA(r1 c1 r2 c2)`, 1-based and inclusive.
    Box(u8, u8, u8, u8),
}

/// One help specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpSpec {
    /// The area.
    pub area: HelpArea,
    /// The help module.
    pub module: String,
    /// The panel group, `LIB/NAME` or `NAME`.
    pub group: String,
}

fn words(k: &KeywordUse) -> Vec<String> {
    let mut out = Vec::new();
    for p in &k.parms {
        match p {
            dds::lex::Parm::Group(g) => {
                out.extend(g.iter().filter_map(|w| w.as_word().map(str::to_string)));
            }
            p => {
                if let Some(w) = p.as_word() {
                    out.push(w.to_string());
                } else if let Some(q) = p.as_quoted() {
                    out.push(q.to_ascii_uppercase());
                }
            }
        }
    }
    out
}

fn area_of(w: &[String]) -> Option<HelpArea> {
    match w.first().map(String::as_str) {
        Some("*RCD") => Some(HelpArea::Record),
        Some("*FLD") => w.get(1).map(|n| HelpArea::Field(n.clone())),
        Some("*CNST") => w
            .get(1)
            .and_then(|n| n.parse().ok())
            .map(HelpArea::Constant),
        _ => {
            let n: Vec<u8> = w.iter().filter_map(|x| x.parse().ok()).collect();
            (n.len() == 4).then(|| HelpArea::Box(n[0], n[1], n[2], n[3]))
        }
    }
}

/// The record's help specifications in source order.
#[must_use]
pub fn specs(rec: &Record) -> Vec<HelpSpec> {
    let mut out = Vec::new();
    let mut area = None;
    for k in &rec.keywords {
        match k.name.as_str() {
            "HLPARA" => area = area_of(&words(k)),
            "HLPPNLGRP" => {
                let w = words(k);
                if let (Some(a), Some(m), Some(g)) = (area.take(), w.first(), w.get(1)) {
                    out.push(HelpSpec {
                        area: a,
                        module: m.clone(),
                        group: g.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// The help specification under the cursor (1-based): the first whose
/// area holds it, else the record's own.
#[must_use]
pub fn help_at(rec: &Record, row: u8, col: u8) -> Option<HelpSpec> {
    let all = specs(rec);
    let constants: Vec<&crate::model::Field> =
        rec.fields.iter().filter(|f| f.constant.is_some()).collect();
    let holds = |f: &crate::model::Field| {
        f.positions.first().is_some_and(|p| {
            p.row == row && col >= p.col && u32::from(col) < u32::from(p.col) + f.screen_len.max(1)
        })
    };
    all.iter()
        .find(|s| match &s.area {
            HelpArea::Record => false,
            HelpArea::Box(r1, c1, r2, c2) => {
                (*r1..=*r2).contains(&row) && (*c1..=*c2).contains(&col)
            }
            HelpArea::Field(n) => rec
                .fields
                .iter()
                .any(|f| f.name.as_deref() == Some(n.as_str()) && holds(f)),
            HelpArea::Constant(n) => (*n as usize)
                .checked_sub(1)
                .and_then(|i| constants.get(i))
                .is_some_and(|f| holds(f)),
        })
        .or_else(|| all.iter().find(|s| s.area == HelpArea::Record))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompileOpts, NoRefs, Source, compile_dspf};

    #[test]
    fn help_specs_name_their_modules() {
        let dds = "
     A                                      DSPSIZ(24 80 *DS3)
     A          R ONE
     A          H                           HLPARA(*RCD)
     A                                      HLPPNLGRP(ONEHLP QSYS/MYGRP)
     A          H                           HLPARA(5 2 5 40)
     A                                      HLPPNLGRP('ONE/NAME' MYGRP)
     A                                  1 30'Title'
     A            NAME          10A  B  5 20
";
        let (src, _) = Source::from_text(&dds[1..]);
        let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
        let f = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
        let rec = &f.records[0];
        assert_eq!(specs(rec).len(), 2);
        assert_eq!(help_at(rec, 5, 21).unwrap().module, "ONE/NAME");
        let any = help_at(rec, 1, 31).unwrap();
        assert_eq!(
            (any.module.as_str(), any.group.as_str()),
            ("ONEHLP", "QSYS/MYGRP")
        );
    }
}
