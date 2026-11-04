// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calculation specifications: the wider factors, the renamed
//! operations, half adjust as `(H)`, array elements in parentheses. A
//! calculation conditioned by more than one indicator becomes lines of
//! one indicator each, the later ones `AN`, as ILE RPG has room for one.

use super::place::{Rec, cut3, factor, opcode, raw3};
use super::{Out, Sect};

/// The three conditioning indicators of columns 9-17.
fn conditions(r: &str) -> Vec<String> {
    let all = raw3("C", r, "cond");
    let chars: Vec<char> = all.chars().collect();
    chars
        .chunks(3)
        .map(|c| c.iter().collect::<String>())
        .filter(|c| !c.trim().is_empty())
        .collect()
}

/// Convert one calculation.
pub(super) fn convert(r: &str, at: usize, out: &mut Out) {
    let level = cut3("C", r, "level");
    let conds = conditions(r);
    let op = cut3("C", r, "opcode");
    let ext = cut3("C", r, "halfadj");
    // Every indicator but the last on a line of its own.
    let (last, before) = conds.split_last().map_or((None, &[][..]), |(l, b)| (Some(l.clone()), b));
    for (k, c) in before.iter().enumerate() {
        let mut line = Rec::new("C", r);
        let lv = if k == 0 { level.clone() } else { "AN".to_string() };
        line.put("level", &lv).put("cond", c);
        out.push(Sect::C, line.text(), at);
    }
    let mut c = Rec::new("C", r);
    let lv = if before.is_empty() { level.clone() } else { "AN".to_string() };
    c.put("level", &lv);
    if let Some(l) = &last {
        c.put("cond", l);
    }
    let mut name = opcode(&op);
    if !ext.is_empty() && !op.is_empty() {
        name = format!("{name}({ext})");
    }
    c.put("factor1", &factor(&cut3("C", r, "factor1")))
        .put("opcode", &name)
        .put("factor2", &factor(&cut3("C", r, "factor2")))
        .put("result", &factor(&cut3("C", r, "result")))
        .right("len", &cut3("C", r, "len"))
        .right("decimals", &cut3("C", r, "decimals"))
        .put("hi", &cut3("C", r, "hi"))
        .put("lo", &cut3("C", r, "lo"))
        .put("eq", &cut3("C", r, "eq"));
    let comment = cut3("C", r, "comments");
    if !comment.is_empty() {
        c.at(81, &comment);
    }
    out.push(Sect::C, c.text(), at);
}
