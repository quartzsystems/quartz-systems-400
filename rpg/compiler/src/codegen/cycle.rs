// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG cycle with a primary file, in IBM's order: heading and
//! detail output; `1P`, the record identifying, control level and `MR`
//! indicators off; the next record read from the file the last one came
//! from (the first cycle reads every file); record selection (matching
//! fields, the primary first on a tie, else the primary to its end and
//! then each secondary); at the end of every file `LR` and `L1`-`L9`
//! on; the record type and its indicator, the control break (a change
//! in a level's fields sets that level and the ones below on) and `MR`;
//! total calculations and output (not on the first cycle); the end when
//! `LR` is on; overflow output; the record's fields; detail
//! calculations.

use rpgfront::ast::{Module, Stmt};

use crate::codegen::Gen;
use crate::codegen::emit::Body;
use crate::codegen::output::Step;
use crate::codegen::stmt;
use crate::files::{Dev, File};
use crate::names::mangle;

/// A field's bytes in its record buffer, with its level or matching
/// number.
#[derive(Clone, Debug)]
struct Slot {
    /// Record type index (`$rt_FILE`), or -1 for every type.
    rt: i64,
    /// Offset in the buffer.
    off: u32,
    /// Bytes.
    len: u32,
    /// `L1`-`L9` or `M1`-`M9`: the number.
    n: u8,
}

/// Whether a module runs the cycle with a primary file.
#[must_use]
pub fn has_primary(g: &Gen<'_>) -> bool {
    g.files.iter().any(|f| f.scope == 0 && f.designation == "P" && f.dev == Dev::Disk)
}

/// The files the cycle reads: the primary, then the secondaries.
fn cycle_files(g: &Gen<'_>) -> Vec<File> {
    let mut v: Vec<File> = g.files.iter().filter(|f| f.scope == 0 && f.designation == "P").cloned().collect();
    v.extend(g.files.iter().filter(|f| f.scope == 0 && f.designation == "S").cloned());
    v
}

/// The control-level or matching-field slots of a file.
fn slots(g: &Gen<'_>, f: &File, matching: bool) -> Vec<Slot> {
    let mut out = Vec::new();
    let pick = |x: &rpgfront::ast::InputField| -> Option<u8> {
        let w = if matching { &x.matching } else { &x.level };
        let p = if matching { 'M' } else { 'L' };
        w.strip_prefix(p).and_then(|d| d.parse::<u8>().ok()).filter(|d| (1..=9).contains(d))
    };
    if let Some(r) = g.t.recfmts.iter().find(|r| r.file_name == f.name) {
        for i in g.inputs.iter().filter(|i| i.external && (i.name == r.name || i.name == r.fmt.name)) {
            for x in &i.fields {
                let Some(n) = pick(x) else { continue };
                if let Some(k) = r.fmt.fields.iter().position(|d| d.name == x.extname) {
                    out.push(Slot {
                        rt: -1,
                        off: r.fmt.fields[k].offset,
                        len: crate::extfiles::field_len(&r.fmt, k),
                        n,
                    });
                }
            }
        }
        return out;
    }
    for (k, i) in g.records_of(f).iter().enumerate() {
        for x in &i.fields {
            let Some(n) = pick(x) else { continue };
            let (Ok(from), Ok(to)) = (x.from.trim().parse::<u32>(), x.to.trim().parse::<u32>()) else { continue };
            out.push(Slot {
                rt: i64::try_from(k).unwrap_or(0),
                off: from - 1,
                len: to + 1 - from,
                n,
            });
        }
    }
    out
}

/// The bytes of a file's slots, highest number first, into a new
/// temporary of `len` bytes (only the record type's slots).
fn gather(g: &mut Gen<'_>, f: &File, slots: &[Slot], len: u32) -> String {
    let n = mangle(&f.name);
    let t = g.b.temp(&format!("char({})", len.max(1)));
    g.b.ins(format!("mov {t}, *BLANKS"));
    let p = g.b.reg();
    g.b.ins(format!("{p} = addr $rb_{n}"));
    let q = g.b.reg();
    g.b.ins(format!("{q} = addr {t}"));
    let rt = g.b.reg();
    g.b.ins(format!("{rt} = load $rt_{n}"));
    let mut sorted = slots.to_vec();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.n));
    let mut at = 0;
    for s in &sorted {
        let skip = g.b.label();
        if s.rt >= 0 {
            let c = g.b.reg();
            g.b.ins(format!("{c} = cmp.eq {rt}, {}", s.rt));
            let go = g.b.label();
            g.b.term(format!("brif {c}, {go}, {skip}"));
            g.b.place(&go);
        }
        if at + s.len <= len {
            g.b.ins(format!("mov [{q} +{at}]:char({}), [{p} +{}]:char({})", s.len, s.off, s.len));
        }
        g.b.place(&skip);
        at += s.len;
    }
    t
}

/// The bytes the slots of one level take (level `n` and above, the
/// order of [`gather`]).
fn level_len(slots: &[Slot], n: u8) -> u32 {
    slots.iter().filter(|s| s.n >= n).map(|s| s.len).sum()
}

/// The main procedure of a module with a primary file.
pub fn main_proc(g: &mut Gen<'_>, m: &Module) -> String {
    g.b = Body::new();
    g.scope = 0;
    g.subrs.clear();
    g.tags.clear();
    g.open.clear();
    let files = cycle_files(g);
    let lvl_slots: Vec<(File, Vec<Slot>)> = files.iter().map(|f| (f.clone(), slots(g, f, false))).collect();
    let mt_slots: Vec<(File, Vec<Slot>)> = files.iter().map(|f| (f.clone(), slots(g, f, true))).collect();
    let mlen = mt_slots.iter().map(|(_, s)| s.iter().map(|x| x.len).sum::<u32>()).max().unwrap_or(0);
    let clen = lvl_slots.iter().map(|(_, s)| s.iter().map(|x| x.len).sum::<u32>()).max().unwrap_or(0);
    // The cycle's own storage.
    g.b.decls.push("static $cy_first: ind".into());
    g.b.decls.push("static $cy_sel: i32".into());
    g.b.decls.push(format!("static $cy_lv: char({})", clen.max(1)));
    for f in &files {
        let n = mangle(&f.name);
        g.b.decls.push(format!("static $cy_has_{n}: ind"));
        g.b.decls.push(format!("static $cy_end_{n}: ind"));
        g.b.decls.push(format!("static $cy_m_{n}: char({})", mlen.max(1)));
        g.b.decls.push(format!("static $cy_ms_{n}: ind"));
    }
    let start = g.b.label();
    let init = g.b.label();
    g.b.ins("%first = load $init_done");
    g.b.term(format!("brif %first, {start}, {init}"));
    g.b.place(&init);
    g.b.ins("callb _qrni_init()");
    let all = g.files;
    for f in all.iter().filter(|f| f.scope == 0 && !f.usropn && !f.template) {
        let _ = g.db_open(f);
    }
    g.init_tail(m);
    g.b.ins("store $init_done, *ON");
    g.psds_init(&g.opts.name.clone());
    g.b.ins("store $cy_first, *ON");
    g.b.ins("store $cy_sel, -1");
    for f in &files {
        let n = mangle(&f.name);
        g.b.ins(format!("store $cy_has_{n}, *OFF"));
        g.b.ins(format!("store $cy_end_{n}, *OFF"));
        g.b.ins(format!("store $cy_ms_{n}, *OFF"));
    }
    g.b.place(&start);
    g.b.ins("store $in1p, *ON");
    let finish = g.b.label();
    g.main_exit = Some(finish.clone());
    let (detail, total): (Vec<&Stmt>, Vec<&Stmt>) = m.calcs.iter().partition(|s| !super::is_total(&s.level));
    let outputs = g.outputs.clone();
    let top = g.b.label();
    let end = g.b.label();
    let ret = g.b.label();
    g.b.place(&top);
    // Heading and detail output.
    g.output(&outputs, Step::HeadingDetail);
    // Indicators off.
    g.b.ins("store $in1p, *OFF");
    g.b.ins("store $inmr, *OFF");
    for k in 1..=9 {
        g.b.ins(format!("store $inl{k}, *OFF"));
    }
    for f in &files {
        let recs = g.records_of(f);
        for r in recs {
            if !r.rid.is_empty() && r.rid != "**" && let Some(x) = g.indicator(&r.rid) {
                g.b.ins(format!("store {}, *OFF", x.op));
            }
        }
        for i in g.inputs.clone().iter().filter(|i| i.external && !i.rid.is_empty()) {
            if g.t.recfmts.iter().any(|r| r.file_name == f.name && (r.name == i.name || r.fmt.name == i.name))
                && let Some(x) = g.indicator(&i.rid)
            {
                g.b.ins(format!("store {}, *OFF", x.op));
            }
        }
    }
    // LR on from the calculations: no record, straight to total time.
    let totals = g.b.label();
    let lr_early = g.b.reg();
    g.b.ins(format!("{lr_early} = load $inlr"));
    let reading = g.b.label();
    g.b.term(format!("brif {lr_early}, {totals}, {reading}"));
    g.b.place(&reading);
    // Read: every file on the first cycle, else the one last selected.
    for (k, f) in files.iter().enumerate() {
        let n = mangle(&f.name);
        let rd = g.b.label();
        let next = g.b.label();
        let first = g.b.reg();
        g.b.ins(format!("{first} = load $cy_first"));
        let sel = g.b.reg();
        g.b.ins(format!("{sel} = load $cy_sel"));
        let mine = g.b.reg();
        g.b.ins(format!("{mine} = cmp.eq {sel}, {k}"));
        let want = g.b.reg();
        g.b.ins(format!("{want} = select {first}, {first}, {mine}"));
        let ended = g.b.reg();
        g.b.ins(format!("{ended} = load $cy_end_{n}"));
        let go = g.b.reg();
        g.b.ins(format!("{go} = select {ended}, *OFF, {want}"));
        g.b.term(format!("brif {go}, {rd}, {next}"));
        g.b.place(&rd);
        let st = g.b.reg();
        g.b.ins(format!("{st} = dbio {n} READ rec($rb_{n})"));
        let ok = g.b.reg();
        g.b.ins(format!("{ok} = cmp.eq {st}, 0"));
        g.b.ins(format!("store $cy_has_{n}, {ok}"));
        let no = g.b.reg();
        g.b.ins(format!("{no} = cmp.eq {ok}, *OFF"));
        g.b.ins(format!("store $cy_end_{n}, {no}"));
        let got = g.b.label();
        g.b.term(format!("brif {ok}, {got}, {next}"));
        g.b.place(&got);
        if g.t.recfmts.iter().any(|r| r.file_name == f.name) {
            g.b.ins(format!("store $rt_{n}, 0"));
        } else {
            g.identify_as(f, false);
        }
        g.b.place(&next);
    }
    // Selection.
    g.b.ins("store $cy_sel, -1");
    let chosen = g.b.label();
    if mlen > 0 {
        // The lowest matching value; the earlier file on a tie.
        let best = g.b.temp(&format!("char({mlen})"));
        for (k, (f, sl)) in mt_slots.iter().enumerate() {
            let n = mangle(&f.name);
            let skip = g.b.label();
            let try_it = g.b.label();
            let has = g.b.reg();
            g.b.ins(format!("{has} = load $cy_has_{n}"));
            g.b.term(format!("brif {has}, {try_it}, {skip}"));
            g.b.place(&try_it);
            let v = gather(g, f, sl, mlen);
            let take = g.b.label();
            let sel = g.b.reg();
            g.b.ins(format!("{sel} = load $cy_sel"));
            let none = g.b.reg();
            g.b.ins(format!("{none} = cmp.lt {sel}, 0"));
            let lower = g.b.reg();
            g.b.ins(format!("{lower} = str.cmp.lt {v}, {best}"));
            let c = g.b.reg();
            g.b.ins(format!("{c} = select {none}, {none}, {lower}"));
            g.b.term(format!("brif {c}, {take}, {skip}"));
            g.b.place(&take);
            g.b.ins(format!("mov {best}, {v}"));
            g.b.ins(format!("store $cy_sel, {k}"));
            g.b.place(&skip);
        }
        // MR: another file holds or held the same value.
        let sel = g.b.reg();
        g.b.ins(format!("{sel} = load $cy_sel"));
        let any = g.b.reg();
        g.b.ins(format!("{any} = cmp.ge {sel}, 0"));
        let mr = g.b.label();
        g.b.term(format!("brif {any}, {mr}, {chosen}"));
        g.b.place(&mr);
        for (k, (f, sl)) in mt_slots.iter().enumerate() {
            let n = mangle(&f.name);
            let skip = g.b.label();
            let other = g.b.reg();
            g.b.ins(format!("{other} = cmp.ne {sel}, {k}"));
            let look = g.b.label();
            g.b.term(format!("brif {other}, {look}, {skip}"));
            g.b.place(&look);
            // Its lookahead record.
            let has = g.b.reg();
            g.b.ins(format!("{has} = load $cy_has_{n}"));
            let la = g.b.label();
            let held = g.b.label();
            g.b.term(format!("brif {has}, {la}, {held}"));
            g.b.place(&la);
            let v = gather(g, f, sl, mlen);
            let eq = g.b.reg();
            g.b.ins(format!("{eq} = str.cmp.eq {v}, {best}"));
            let on = g.b.label();
            g.b.term(format!("brif {eq}, {on}, {held}"));
            g.b.place(&on);
            g.b.ins("store $inmr, *ON");
            g.b.term(format!("br {held}"));
            // The record it last gave.
            g.b.place(&held);
            let ms = g.b.reg();
            g.b.ins(format!("{ms} = load $cy_ms_{n}"));
            let cmp = g.b.label();
            g.b.term(format!("brif {ms}, {cmp}, {skip}"));
            g.b.place(&cmp);
            let eq2 = g.b.reg();
            g.b.ins(format!("{eq2} = str.cmp.eq $cy_m_{n}, {best}"));
            let on2 = g.b.label();
            g.b.term(format!("brif {eq2}, {on2}, {skip}"));
            g.b.place(&on2);
            g.b.ins("store $inmr, *ON");
            g.b.place(&skip);
        }
        // The selected file remembers its value.
        for (k, (f, _)) in mt_slots.iter().enumerate() {
            let n = mangle(&f.name);
            let c = g.b.reg();
            g.b.ins(format!("{c} = cmp.eq {sel}, {k}"));
            let yes = g.b.label();
            let no = g.b.label();
            g.b.term(format!("brif {c}, {yes}, {no}"));
            g.b.place(&yes);
            g.b.ins(format!("mov $cy_m_{n}, {best}"));
            g.b.ins(format!("store $cy_ms_{n}, *ON"));
            g.b.place(&no);
        }
        g.b.term(format!("br {chosen}"));
    } else {
        // The primary to its end, then each secondary.
        for (k, f) in files.iter().enumerate() {
            let n = mangle(&f.name);
            let next = g.b.label();
            let has = g.b.reg();
            g.b.ins(format!("{has} = load $cy_has_{n}"));
            let take = g.b.label();
            g.b.term(format!("brif {has}, {take}, {next}"));
            g.b.place(&take);
            g.b.ins(format!("store $cy_sel, {k}"));
            g.b.term(format!("br {chosen}"));
            g.b.place(&next);
        }
    }
    g.b.place(&chosen);
    let sel = g.b.reg();
    g.b.ins(format!("{sel} = load $cy_sel"));
    let none = g.b.reg();
    g.b.ins(format!("{none} = cmp.lt {sel}, 0"));
    let eof = g.b.label();
    let rec = g.b.label();
    g.b.term(format!("brif {none}, {eof}, {rec}"));
    g.b.place(&eof);
    g.b.ins("store $inlr, *ON");
    for k in 1..=9 {
        g.b.ins(format!("store $inl{k}, *ON"));
    }
    g.b.term(format!("br {totals}"));
    // The selected record: its indicator, the control break.
    g.b.place(&rec);
    for (k, (f, sl)) in lvl_slots.iter().enumerate() {
        let skip = g.b.label();
        let mine = g.b.reg();
        g.b.ins(format!("{mine} = cmp.eq {sel}, {k}"));
        let go = g.b.label();
        g.b.term(format!("brif {mine}, {go}, {skip}"));
        g.b.place(&go);
        g.set_rid(f);
        // External record identifying indicators.
        for i in g.inputs.clone().iter().filter(|i| i.external && !i.rid.is_empty()) {
            if g.t.recfmts.iter().any(|r| r.file_name == f.name && (r.name == i.name || r.fmt.name == i.name))
                && let Some(x) = g.indicator(&i.rid)
            {
                g.b.ins(format!("store {}, *ON", x.op));
            }
        }
        if clen > 0 && !sl.is_empty() {
            let v = gather(g, f, sl, clen);
            let first = g.b.reg();
            g.b.ins(format!("{first} = load $cy_first"));
            // Levels from the highest down: a change sets it and those below.
            let levels: Vec<u8> = (1..=9u8).rev().filter(|l| sl.iter().any(|s| s.n == *l)).collect();
            let done = g.b.label();
            for l in &levels {
                let len = level_len(sl, *l);
                let next = g.b.label();
                let p = g.b.reg();
                g.b.ins(format!("{p} = addr {v}"));
                let q = g.b.reg();
                g.b.ins(format!("{q} = addr $cy_lv"));
                let ne = g.b.reg();
                g.b.ins(format!("{ne} = str.cmp.ne [{p} +0]:char({len}), [{q} +0]:char({len})"));
                let brk = g.b.reg();
                g.b.ins(format!("{brk} = select {first}, {first}, {ne}"));
                let on = g.b.label();
                g.b.term(format!("brif {brk}, {on}, {next}"));
                g.b.place(&on);
                for j in 1..=*l {
                    g.b.ins(format!("store $inl{j}, *ON"));
                }
                g.b.term(format!("br {done}"));
                g.b.place(&next);
            }
            g.b.place(&done);
            g.b.ins(format!("mov $cy_lv, {v}"));
        }
        g.b.place(&skip);
    }
    g.b.place(&totals);
    // Total time, after the first cycle.
    let tot = g.b.label();
    let after_tot = g.b.label();
    let first = g.b.reg();
    g.b.ins(format!("{first} = load $cy_first"));
    g.b.term(format!("brif {first}, {after_tot}, {tot}"));
    g.b.place(&tot);
    total_calcs(g, &total);
    g.output(&outputs, Step::Total);
    g.b.place(&after_tot);
    g.b.ins("store $cy_first, *OFF");
    let lr = g.b.reg();
    g.b.ins(format!("{lr} = load $inlr"));
    let detail_l = g.b.label();
    g.b.term(format!("brif {lr}, {end}, {detail_l}"));
    g.b.place(&detail_l);
    // Overflow output.
    g.overflow_output(&outputs);
    // The record's fields.
    let sel = g.b.reg();
    g.b.ins(format!("{sel} = load $cy_sel"));
    for (k, f) in files.iter().enumerate() {
        let n = mangle(&f.name);
        let skip = g.b.label();
        let mine = g.b.reg();
        g.b.ins(format!("{mine} = cmp.eq {sel}, {k}"));
        let go = g.b.label();
        g.b.term(format!("brif {mine}, {go}, {skip}"));
        g.b.place(&go);
        match g.t.recfmts.iter().find(|r| r.file_name == f.name).cloned() {
            Some(r) => {
                g.fields_in(&n, &r);
            }
            None => g.move_fields(f),
        }
        g.b.place(&skip);
    }
    stmt::calcs(g, &detail);
    let rt = g.b.reg();
    g.b.ins(format!("{rt} = load $inrt"));
    g.b.term(format!("brif {rt}, {ret}, {top}"));
    g.b.place(&finish);
    let lr2 = g.b.reg();
    g.b.ins(format!("{lr2} = load $inlr"));
    g.b.term(format!("brif {lr2}, {end}, {ret}"));
    g.b.place(&end);
    for f in all.iter().filter(|f| f.scope == 0 && !f.template) {
        let r = g.b.reg();
        g.b.ins(format!("{r} = dbclose {}", mangle(&f.name)));
    }
    g.b.ins("store $init_done, *OFF");
    g.b.term("ret");
    g.b.place(&ret);
    g.b.ins("store $inrt, *OFF");
    g.b.term("ret");
    stmt::finish_subrs(g);
    let body = std::mem::take(&mut g.b);
    body.finish(&format!("proc {}()", mangle(&g.opts.name)), "ret")
}

/// Total calculations: runs of the same level, each under its level
/// indicator (`L0` always, `LR` and `L1`-`L9` by theirs).
fn total_calcs(g: &mut Gen<'_>, total: &[&Stmt]) {
    let mut k = 0;
    while k < total.len() {
        let level = total[k].level.clone();
        let mut e = k;
        while e < total.len() && total[e].level == level {
            e += 1;
        }
        let run: Vec<&Stmt> = total[k..e].to_vec();
        if level == "L0" {
            stmt::calcs(g, &run);
        } else {
            let ind = format!("$in{}", level.to_ascii_lowercase());
            let r = g.b.reg();
            g.b.ins(format!("{r} = load {ind}"));
            let go = g.b.label();
            let skip = g.b.label();
            g.b.term(format!("brif {r}, {go}, {skip}"));
            g.b.place(&go);
            stmt::calcs(g, &run);
            g.b.place(&skip);
        }
        k = e;
    }
}
