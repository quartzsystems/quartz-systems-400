// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fixed-form specifications into syntax nodes. Each record is cut
//! with the shared tables (`rpgspecs`); keyword areas and extended
//! factor 2 continue over the lines the item holds.

use rpgspecs::{Form, cut, form};

use crate::ast::{
    CondInd, Conditioning, Def, DefCols, DefKind, Expr, FileCols, FileDecl, Input, InputField, Keyword,
    Lit, Op, Output, OutputField, Pos, Proc, RecCode, Stmt,
};
use crate::diag::{Diag, ids};
use crate::lex::{self, Segment};
use crate::parse::{Builder, FixedSpec, expr, kw, ops};
use crate::reader::Line;

fn f(id: &str) -> &'static Form {
    form(id).expect("the spec table has the form")
}

/// The operation code of a `C` record, upper case, without extenders.
#[must_use]
pub fn opcode_of(text: &str) -> String {
    let raw = cut(text, 26, 35).trim().to_ascii_uppercase();
    raw.split('(').next().unwrap_or("").trim().to_string()
}

/// The keyword area of an item's lines from `col` to 80, as one token
/// stream (literals continue across lines).
fn keyword_area(b: &mut Builder<'_>, lines: &[&Line], col: usize) -> Vec<Keyword> {
    let mut areas: Vec<(u32, String)> = lines
        .iter()
        .map(|l| (l.no, cut(&l.text, col, 80).trim_end().to_string()))
        .collect();
    // A keyword the entry's end (column 80) cuts off is ignored.
    if let (Some(l), Some((_, t))) = (lines.last(), areas.last_mut()) {
        let open = t.matches('(').count() > t.matches(')').count();
        let beyond = !cut(&l.text, 81, 100).trim().is_empty();
        if open && beyond {
            let start = t.rfind('(').and_then(|p| t[..p].rfind(' ')).map_or(0, |p| p + 1);
            t.truncate(start);
            b.diags.push(Diag::new(ids::KEYWORD_CUT, 20, l.pos(80)));
        }
    }
    let segs: Vec<Segment<'_>> = areas
        .iter()
        .map(|(no, t)| Segment {
            line: *no,
            col: u16::try_from(col).unwrap_or(44),
            text: t.as_str(),
        })
        .collect();
    let toks = lex::lex(&segs, &mut b.diags);
    let end = lines.last().map_or(Pos::default(), |l| l.pos(80));
    kw::keywords(&toks, end, &mut b.diags)
}

/// Operands of one fixed-form factor field.
fn factor(b: &mut Builder<'_>, l: &Line, from: usize, to: usize) -> Vec<Expr> {
    let mut text = cut(&l.text, from, to);
    if text.trim().is_empty() {
        return Vec::new();
    }
    // Digits run into a name (`9N98`) are not valid data: the name is
    // used.
    let t = text.trim_start();
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let rest = &t[digits..];
    let exponent = rest.starts_with(['E', 'e']) && rest[1..].starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '-');
    if digits > 0 && rest.starts_with(|c: char| c.is_ascii_alphabetic()) && !exponent {
        b.diags.push(Diag::new(ids::ENTRY_DATA, 20, l.pos(u16::try_from(from).unwrap_or(0))));
        let lead = text.len() - t.len() + digits;
        text = format!("{}{}", " ".repeat(lead), &text[lead..]);
    }
    let at = l.pos(u16::try_from(from).unwrap_or(0));
    let toks = lex::lex_text(&text, at, &mut b.diags);
    expr::operands(&toks, l.pos(u16::try_from(to).unwrap_or(0)), &mut b.diags)
}

/// One conditioning indicator from columns 9-11 (or an output line's
/// three-column slot).
fn cond_ind(s: &str) -> Option<CondInd> {
    let s3: Vec<char> = s.chars().collect();
    let not = s3.first().is_some_and(|c| c.eq_ignore_ascii_case(&'N'));
    let ind: String = s3.iter().skip(1).collect::<String>().trim().to_ascii_uppercase();
    if ind.is_empty() {
        return None;
    }
    Some(CondInd { not, ind })
}

/// Add a line's conditioning to a set: `AN` joins the last group, `OR`
/// starts one, anything else starts the first.
fn add_cond(c: &mut Conditioning, level: &str, ind: Option<CondInd>) {
    let Some(ind) = ind else { return };
    if level == "AN" && !c.is_empty() {
        c.last_mut().expect("a group").push(ind);
    } else {
        c.push(vec![ind]);
    }
}

/// Handle one fixed-form item. `group` says whether a `DS`, `PR` or
/// `PI` line is open for subfields; the result says whether one is now.
pub fn spec(b: &mut Builder<'_>, spec: &FixedSpec, group: bool) -> bool {
    let lines: Vec<&Line> = spec.lines.iter().map(|&i| &b.lines[i]).collect();
    let first = lines[0];
    match spec.form {
        'H' => {
            let k = keyword_area(b, &lines, 7);
            b.module.ctl.extend(k);
            false
        }
        'F' => {
            file_spec(b, &lines);
            false
        }
        'D' => def_spec(b, &lines, group),
        'P' => {
            proc_spec(b, &lines);
            false
        }
        'C' => {
            calc_spec(b, &lines);
            false
        }
        'I' => {
            input_spec(b, &lines);
            false
        }
        'O' => {
            output_spec(b, &lines);
            false
        }
        other => {
            b.diags
                .push(Diag::new(ids::FORM_SEQUENCE, 30, first.pos(6)).arg(other.to_string()));
            group
        }
    }
}

/// A numeric entry must end in its field's last column (RNF0263).
fn right_adjusted(b: &mut Builder<'_>, l: &Line, form: &Form, field: &str) {
    let Some(fd) = form.field(field) else { return };
    let raw = cut(&l.text, fd.from, fd.to);
    if !raw.trim().is_empty() && raw.ends_with(' ') {
        let col = fd.from + raw.len() - raw.trim_start().len();
        b.diags.push(Diag::new(ids::NOT_RIGHT_ADJUSTED, 20, l.pos(u16::try_from(col).unwrap_or(0))));
    }
}

fn file_spec(b: &mut Builder<'_>, lines: &[&Line]) {
    let l = lines[0];
    let ff = f("F");
    right_adjusted(b, l, ff, "reclen");
    right_adjusted(b, l, ff, "keylen");
    if !cut(&l.text, 43, 43).trim().is_empty() {
        b.diags.push(Diag::new(ids::F_COL43, 20, l.pos(43)));
    }
    let c = |n: &str| ff.cut(&l.text, n).trim().to_ascii_uppercase();
    let cols = FileCols {
        ftype: c("type"),
        designation: c("designation"),
        eof: c("eof"),
        addition: c("addition"),
        sequence: c("sequence"),
        format: c("format"),
        reclen: c("reclen"),
        limits: c("limits"),
        keylen: c("keylen"),
        rectype: c("rectype"),
        org: c("org"),
        device: c("device"),
    };
    let kws = keyword_area(b, lines, 44);
    b.file(FileDecl {
        name: c("name"),
        fixed: Some(cols),
        kws,
        at: l.pos(7),
    });
}

/// The name of a `D` or `P` item: continued over `...` lines.
fn long_name(lines: &[&Line]) -> (String, usize) {
    let mut name = String::new();
    let mut k = 0;
    while k < lines.len() {
        let t = cut(&lines[k].text, 7, 80);
        let t = t.trim();
        if t.ends_with("...") && !t.contains(' ') && k + 1 < lines.len() {
            name.push_str(t.trim_end_matches("..."));
            k += 1;
            continue;
        }
        break;
    }
    let last = lines.get(k).map_or(String::new(), |l| cut(&l.text, 7, 21).trim().to_string());
    name.push_str(last.trim_end_matches("..."));
    (name, k)
}

fn def_spec(b: &mut Builder<'_>, lines: &[&Line], group: bool) -> bool {
    let (name, k) = long_name(lines);
    let l = lines[k];
    let df = f("D");
    right_adjusted(b, l, df, "from");
    right_adjusted(b, l, df, "to");
    right_adjusted(b, l, df, "decimals");
    let c = |n: &str| df.cut(&l.text, n).trim().to_ascii_uppercase();
    let deftype = c("deftype");
    let cols = DefCols {
        ext: c("ext"),
        dstype: c("dstype"),
        from: c("from"),
        to: c("to"),
        ty: c("type"),
        decimals: c("decimals"),
    };
    let before = b.diags.len();
    let kws = keyword_area(b, &lines[k..], 44);
    let kind = match deftype.as_str() {
        "S" => DefKind::Standalone,
        "C" => DefKind::Const,
        "DS" => DefKind::Ds,
        "PR" => DefKind::Pr,
        "PI" => DefKind::Pi,
        "" if group => DefKind::Subfield,
        "" => {
            // A subfield outside a group is a standalone field.
            b.diags.push(Diag::new(ids::NOT_IN_GROUP, 20, l.pos(7)));
            DefKind::Standalone
        }
        _ => {
            // IBM takes a definition type it does not know as S.
            b.diags.push(Diag::new(ids::BAD_DEF_TYPE, 20, l.pos(24)).arg(deftype));
            DefKind::Standalone
        }
    };
    let mut d = Def {
        kind,
        name,
        fixed: Some(cols),
        kws,
        at: l.pos(7),
        ..Def::default()
    };
    if kind == DefKind::Const {
        // A constant's value is its CONST keyword or the whole area.
        if let Some(k) = d.kw("CONST").cloned() {
            d.value = k.arg(0).and_then(|a| a.expr.clone());
            d.kws.retain(|x| x.name != "CONST");
        } else {
            // The area is the value, not keywords.
            b.diags.truncate(before);
            let areas: Vec<(u32, String)> = lines[k..]
                .iter()
                .map(|l| (l.no, cut(&l.text, 44, 80).trim_end().to_string()))
                .collect();
            let segs: Vec<Segment<'_>> = areas
                .iter()
                .map(|(no, t)| Segment {
                    line: *no,
                    col: 44,
                    text: t.as_str(),
                })
                .collect();
            let toks = lex::lex(&segs, &mut Vec::new());
            // No value at all is the compiler's RNF3905, not a syntax error.
            if !toks.is_empty() {
                d.value = expr::expr_all(&toks, l.pos(80), &mut b.diags);
            }
            d.kws.clear();
        }
    }
    match kind {
        DefKind::Subfield => {
            let parent = b.last_group();
            match parent {
                Some(p) => {
                    let child_kind = if matches!(p.kind, DefKind::Pr | DefKind::Pi) {
                        DefKind::Parm
                    } else {
                        DefKind::Subfield
                    };
                    p.children.push(Def { kind: child_kind, ..d });
                }
                None => b
                    .diags
                    .push(Diag::new(ids::FORM_SEQUENCE, 30, l.pos(7)).arg("D".to_string())),
            }
            true
        }
        DefKind::Ds | DefKind::Pr | DefKind::Pi => {
            b.def(d);
            true
        }
        _ => {
            b.def(d);
            false
        }
    }
}

fn proc_spec(b: &mut Builder<'_>, lines: &[&Line]) {
    let (name, k) = long_name(lines);
    let l = lines[k];
    let begin = f("P").cut(&l.text, "begin").trim().to_ascii_uppercase();
    let kws = keyword_area(b, &lines[k..], 44);
    match begin.as_str() {
        "B" => {
            if let Some(open) = b.proc.take() {
                b.diags.push(Diag::new(ids::PROC_END_MISSING, 20, open.at));
                b.module.procs.push(open);
            }
            b.after_proc = true;
            b.proc = Some(Proc {
                name,
                kws,
                at: l.pos(7),
                ..Proc::default()
            });
        }
        "E" => b.end_proc(l.pos(24)),
        _ => b.diags.push(Diag::new(ids::BAD_KEYWORD, 30, l.pos(24)).arg(begin)),
    }
}

/// Parse the `FOR` operand: `i = start [TO|DOWNTO limit] [BY step]`.
pub fn for_operand(toks: &[lex::Tok], end: Pos, diags: &mut Vec<Diag>) -> (Option<Expr>, Vec<(String, Expr)>) {
    // Cut at the words TO, DOWNTO and BY outside parentheses.
    let mut parts: Vec<(String, Vec<lex::Tok>)> = vec![(String::new(), Vec::new())];
    let mut depth = 0;
    for t in toks {
        if t.is("(") {
            depth += 1;
        } else if t.is(")") {
            depth -= 1;
        }
        if depth == 0
            && let Some(w) = t.name()
            && matches!(w, "TO" | "DOWNTO" | "BY")
        {
            parts.push((w.to_string(), Vec::new()));
            continue;
        }
        parts.last_mut().expect("a part").1.push(t.clone());
    }
    let mut c = expr::Cursor::new(&parts[0].1, end);
    let init = expr::assignment(&mut c, diags);
    let more = parts[1..]
        .iter()
        .filter_map(|(w, t)| expr::expr_all(t, end, diags).map(|e| (w.clone(), e)))
        .collect();
    (init, more)
}

/// The extended factor 2 of an operation, from its tokens.
pub fn extended_operand(code: &str, toks: &[lex::Tok], end: Pos, diags: &mut Vec<Diag>) -> (Option<Expr>, Vec<(String, Expr)>) {
    match code {
        "EVAL" | "EVALR" | "EVAL-CORR" => {
            let mut c = expr::Cursor::new(toks, end);
            let e = expr::assignment(&mut c, diags);
            if !c.done() {
                diags.push(Diag::new(ids::END_OF_EXPRESSION, 30, c.pos()));
            }
            (e, Vec::new())
        }
        "FOR" => for_operand(toks, end, diags),
        // Blank-separated operands: type, message, %TARGET.
        "SND-MSG" => {
            let mut c = expr::Cursor::new(toks, end);
            let mut list = Vec::new();
            while !c.done() {
                match expr::expr(&mut c, diags) {
                    Some(e) => list.push(e),
                    None => c.skip_all(),
                }
            }
            (Some(Expr::List(list, toks.first().map_or(end, |t| t.at))), Vec::new())
        }
        "ON-ERROR" | "ON-EXCP" | "XML-INTO" | "XML-SAX" | "DATA-INTO" | "DATA-GEN" | "WHEN-IN" => {
            let list = expr::operands(toks, end, diags);
            (Some(Expr::List(list, toks.first().map_or(end, |t| t.at))), Vec::new())
        }
        _ if toks.is_empty() => (None, Vec::new()),
        _ => (expr::expr_all(toks, end, diags), Vec::new()),
    }
}

/// The written operation code and its extenders: `CHAIN(EN)`.
fn split_ext(raw: &str) -> (String, String) {
    let up = raw.trim().to_ascii_uppercase();
    match up.split_once('(') {
        Some((c, e)) => (c.trim().to_string(), e.trim_end_matches(')').trim().to_string()),
        None => (up, String::new()),
    }
}

fn calc_spec(b: &mut Builder<'_>, lines: &[&Line]) {
    let cf = f("C");
    // Conditioning lines first, then the operation line, then the
    // extended factor 2's continuation lines.
    let op_at = lines
        .iter()
        .position(|l| !cut(&l.text, 12, 80).trim().is_empty())
        .unwrap_or(0);
    let l = lines[op_at];
    let mut cond: Conditioning = Vec::new();
    let mut level = String::new();
    for (i, c) in lines[..=op_at].iter().enumerate() {
        let lv = cf.cut(&c.text, "level").trim().to_ascii_uppercase();
        if i == 0 && lv != "AN" && lv != "OR" {
            level.clone_from(&lv);
        }
        let raw: Vec<char> = cf.cut(&c.text, "cond").chars().collect();
        let mut ind = cond_ind(&raw.iter().collect::<String>());
        // Column 9 is N or blank; columns 10-11 an indicator.
        if raw.first().is_some_and(|ch| *ch != ' ' && !ch.eq_ignore_ascii_case(&'N')) {
            b.diags.push(Diag::new(ids::COND_NOT, 20, c.pos(9)));
            ind = cond_ind(&format!(" {}", raw.iter().skip(1).collect::<String>()));
        }
        if let Some(x) = &ind
            && !valid_indicator(&x.ind)
        {
            b.diags.push(Diag::new(ids::COND_IND, 20, c.pos(10)));
            continue;
        }
        add_cond(&mut cond, &lv, ind);
    }
    let (code, mut ext) = split_ext(&cf.cut(&l.text, "opcode"));
    // Extenders that are not known, or not the operation's, are ignored.
    if !ops::extenders_known(&ext) {
        b.diags.push(Diag::new(ids::BAD_EXTENDER, 20, l.pos(26)));
        ext.clear();
    } else if !ops::extenders_allowed(&code, &ext) {
        b.diags.push(Diag::new(ids::EXTENDER_NOT_FOR_OP, 20, l.pos(26)));
        ext.clear();
    }
    let mut op = Op {
        code: code.clone(),
        ext,
        ..Op::default()
    };
    if code.is_empty() {
        b.diags.push(Diag::new(ids::BAD_OPCODE, 30, l.pos(26)).arg(String::new()));
        return;
    }
    if !ops::known(&code) || ops::free_only(&code) {
        b.diags.push(Diag::new(ids::BAD_FIXED_OPCODE, 30, l.pos(26)).arg(code.clone()));
        return;
    }
    op.f1 = factor(b, l, 12, 25);
    if op.f1.is_empty() && ops::needs_factor1(&code) {
        b.diags.push(Diag::new(ids::NO_FACTOR1, 30, l.pos(12)));
    }
    if !op.f1.is_empty() && ops::no_factor1(&code) {
        b.diags.push(Diag::new(ids::FACTOR1_NOT_BLANK, 20, l.pos(12)));
    }
    if ops::extended(&code) {
        let areas: Vec<(u32, String)> = lines[op_at..]
            .iter()
            .map(|x| (x.no, cut(&x.text, 36, 80).trim_end().to_string()))
            .collect();
        let segs: Vec<Segment<'_>> = areas
            .iter()
            .map(|(no, t)| Segment {
                line: *no,
                col: 36,
                text: t.as_str(),
            })
            .collect();
        let toks = lex::lex(&segs, &mut b.diags);
        let end = lines.last().map_or(l.pos(80), |x| x.pos(80));
        let (x, more) = extended_operand(&code, &toks, end, &mut b.diags);
        op.xf2 = x;
        op.more = more;
    } else {
        right_adjusted(b, l, cf, "len");
        right_adjusted(b, l, cf, "decimals");
        op.f2 = factor(b, l, 36, 49);
        op.result = factor(b, l, 50, 63);
        op.len = cf.cut(&l.text, "len").trim().to_string();
        op.decimals = cf.cut(&l.text, "decimals").trim().to_string();
        if !op.decimals.is_empty() && op.decimals.parse::<u8>().ok().is_none_or(|d| d > 63) {
            b.diags.push(Diag::new(ids::BAD_DECIMALS, 20, l.pos(69)));
            op.decimals = "0".into();
        }
        op.resind = [
            cf.cut(&l.text, "hi").trim().to_ascii_uppercase(),
            cf.cut(&l.text, "lo").trim().to_ascii_uppercase(),
            cf.cut(&l.text, "eq").trim().to_ascii_uppercase(),
        ];
        // An indicator that is not one is blanks.
        for (r, col) in op.resind.iter_mut().zip([71u16, 73, 75]) {
            // 1P is an indicator, but no operation sets it.
            if r == "1P" {
                b.diags.push(Diag::new(ids::NOT_RESULT_IND, 20, l.pos(col)));
                r.clear();
            } else if !r.is_empty() && !valid_indicator(r) {
                b.diags.push(Diag::new(ids::BAD_RESULT_IND, 20, l.pos(col)));
                r.clear();
            }
        }
        if matches!(code.as_str(), "SETON" | "SETOFF") && op.resind.iter().all(String::is_empty) {
            b.diags.push(Diag::new(ids::NO_RESULT_IND, 30, l.pos(71)));
        }
    }
    // A detail calculation after a total one is at the total's level.
    let prev = match &b.proc {
        Some(p) => p.calcs.last(),
        None => b.module.calcs.last(),
    };
    if let Some(p) = prev
        && level.is_empty()
        && code != "BEGSR"
        && (p.level == "LR" || p.level.len() == 2 && p.level.starts_with('L') && p.level.as_bytes()[1].is_ascii_digit())
    {
        b.diags.push(Diag::new(ids::LEVEL_BLANK_AFTER, 20, l.pos(7)));
        level.clone_from(&p.level);
    }
    b.stmt(Stmt {
        at: l.pos(26),
        level,
        cond,
        op,
        fixed: !ops::extended(&code),
    });
}

fn input_spec(b: &mut Builder<'_>, lines: &[&Line]) {
    let l = lines[0];
    let (ir, ix, irx, ifx) = (f("I"), f("IX"), f("IR"), f("IF"));
    let name = ir.cut(&l.text, "name").trim().to_ascii_uppercase();
    // An external record line has nothing in columns 17-20 and 23-46.
    let external = cut(&l.text, 17, 20).trim().is_empty() && cut(&l.text, 23, 46).trim().is_empty();
    let name = if name.is_empty() {
        b.module.inputs.last().map(|i| i.name.clone()).unwrap_or_default()
    } else {
        name
    };
    let mut input = Input {
        name,
        external,
        sequence: ir.cut(&l.text, "sequence").trim().to_ascii_uppercase(),
        number: ir.cut(&l.text, "number").trim().to_ascii_uppercase(),
        option: ir.cut(&l.text, "option").trim().to_ascii_uppercase(),
        rid: if external {
            irx.cut(&l.text, "rid").trim().to_ascii_uppercase()
        } else {
            ir.cut(&l.text, "rid").trim().to_ascii_uppercase()
        },
        at: l.pos(7),
        ..Input::default()
    };
    let codes_of = |text: &str| -> Vec<RecCode> {
        (1..=3)
            .filter_map(|n| {
                let pos: u32 = ir.cut(text, &format!("pos{n}")).trim().parse().ok()?;
                Some(RecCode {
                    pos,
                    not: ir.cut(text, &format!("not{n}")).trim().eq_ignore_ascii_case("N"),
                    part: ir.cut(text, &format!("part{n}")).chars().next().unwrap_or('C').to_ascii_uppercase(),
                    ch: ir.cut(text, &format!("char{n}")).chars().next().unwrap_or(' '),
                })
            })
            .collect()
    };
    if !external {
        input.codes.push(codes_of(&l.text));
    }
    for x in &lines[1..] {
        let rel = cut(&x.text, 16, 18).trim().to_ascii_uppercase();
        if rel == "AND" || rel == "OR" {
            let codes = codes_of(&x.text);
            if rel == "AND" {
                if let Some(last) = input.codes.last_mut() {
                    last.extend(codes);
                }
            } else {
                input.codes.push(codes);
            }
            continue;
        }
        let t = &x.text;
        let field = if external {
            InputField {
                extname: ifx.cut(t, "extname").trim().to_ascii_uppercase(),
                name: ifx.cut(t, "field").trim().to_ascii_uppercase(),
                level: ifx.cut(t, "level").trim().to_ascii_uppercase(),
                matching: ifx.cut(t, "match").trim().to_ascii_uppercase(),
                inds: [
                    ifx.cut(t, "plus").trim().to_ascii_uppercase(),
                    ifx.cut(t, "minus").trim().to_ascii_uppercase(),
                    ifx.cut(t, "zero").trim().to_ascii_uppercase(),
                ],
                at: x.pos(21),
                ..InputField::default()
            }
        } else {
            InputField {
                name: ix.cut(t, "field").trim().to_ascii_uppercase(),
                attr: ix.cut(t, "attr").trim().to_ascii_uppercase(),
                sep: ix.cut(t, "sep").trim().to_string(),
                format: ix.cut(t, "format").trim().to_ascii_uppercase(),
                from: ix.cut(t, "from").trim().to_string(),
                to: ix.cut(t, "to").trim().to_string(),
                decimals: ix.cut(t, "decimals").trim().to_string(),
                level: ix.cut(t, "level").trim().to_ascii_uppercase(),
                matching: ix.cut(t, "match").trim().to_ascii_uppercase(),
                relation: ix.cut(t, "relation").trim().to_ascii_uppercase(),
                inds: [
                    ix.cut(t, "plus").trim().to_ascii_uppercase(),
                    ix.cut(t, "minus").trim().to_ascii_uppercase(),
                    ix.cut(t, "zero").trim().to_ascii_uppercase(),
                ],
                at: x.pos(31),
                ..InputField::default()
            }
        };
        input.fields.push(field);
    }
    b.module.inputs.push(input);
}

/// The three conditioning slots of an output line (columns 21-29).
fn output_cond(b: &mut Builder<'_>, l: &Line, c: &mut Conditioning, relation: &str) {
    let text = &l.text;
    let mut inds: Vec<CondInd> = Vec::new();
    for k in 0..3 {
        let raw = cut(text, 21 + 3 * k, 23 + 3 * k);
        let Some(ind) = cond_ind(&raw) else { continue };
        // N or blank, then an indicator; anything else is blanks.
        if !raw.starts_with([' ', 'N', 'n']) || !(valid_indicator(&ind.ind) || ind.ind == "1P") {
            b.diags.push(Diag::new(ids::BAD_OUTPUT_IND, 20, l.pos(u16::try_from(21 + 3 * k).unwrap_or(21))));
            continue;
        }
        inds.push(ind);
    }
    if inds.is_empty() {
        return;
    }
    if relation == "AND" && !c.is_empty() {
        c.last_mut().expect("a group").extend(inds);
    } else {
        c.push(inds);
    }
}

fn output_spec(b: &mut Builder<'_>, lines: &[&Line]) {
    let l = lines[0];
    let (or, of) = (f("O"), f("OF"));
    let t = &l.text;
    let name = or.cut(t, "name").trim().to_ascii_uppercase();
    let name = if name.is_empty() {
        b.module.outputs.last().map(|o| o.name.clone()).unwrap_or_default()
    } else {
        name
    };
    let mut kind = or.cut(t, "type").trim().to_ascii_uppercase();
    if !kind.is_empty() && !matches!(kind.as_str(), "H" | "D" | "T" | "E") {
        b.diags.push(Diag::new(ids::BAD_OUTPUT_TYPE, 20, l.pos(17)));
        kind = "D".into();
    }
    let mut out = Output {
        name,
        kind,
        fetch: cut(t, 18, 20).trim().to_ascii_uppercase(),
        except: or.cut(t, "except").trim().to_ascii_uppercase(),
        space: [
            or.cut(t, "spaceb").trim().to_string(),
            or.cut(t, "spacea").trim().to_string(),
            or.cut(t, "skipb").trim().to_string(),
            or.cut(t, "skipa").trim().to_string(),
        ],
        at: l.pos(7),
        ..Output::default()
    };
    let mut cond = Vec::new();
    output_cond(b, l, &mut cond, "");
    out.cond = cond;
    for x in &lines[1..] {
        let t = &x.text;
        let rel = cut(t, 16, 18).trim().to_ascii_uppercase();
        if rel == "AND" || rel == "OR" {
            let mut c = std::mem::take(&mut out.cond);
            output_cond(b, x, &mut c, &rel);
            out.cond = c;
            continue;
        }
        let mut cond = Vec::new();
        output_cond(b, x, &mut cond, "");
        right_adjusted(b, x, of, "end");
        // A data format IBM does not know is blank.
        let mut format = of.cut(t, "format").trim().to_ascii_uppercase();
        if !format.is_empty() && !"BFILPRU".contains(format.as_str()) {
            b.diags.push(Diag::new(ids::BAD_OUTPUT_FORMAT, 20, x.pos(52)));
            format.clear();
        }
        let constant_text = of.cut(t, "constant");
        let constant = if constant_text.trim().is_empty() {
            None
        } else {
            let toks = lex::lex_text(constant_text.trim_end(), x.pos(53), &mut b.diags);
            expr::expr_all(&toks, x.pos(80), &mut b.diags).or_else(|| Some(Expr::Lit(Lit::Char(constant_text.trim().to_string()), x.pos(53))))
        };
        // A hexadecimal constant needs whole bytes; the field is ignored.
        if let Some(Expr::Lit(Lit::Hex(h), _)) = &constant
            && (h.is_empty() || h.len() % 2 != 0)
        {
            b.diags.push(Diag::new(ids::BAD_OUTPUT_CONSTANT, 30, x.pos(53)));
            continue;
        }
        out.fields.push(OutputField {
            cond,
            field: of.cut(t, "field").trim().to_ascii_uppercase(),
            edit: of.cut(t, "editcode").trim().to_ascii_uppercase(),
            blank: of.cut(t, "blank").trim().eq_ignore_ascii_case("B"),
            end: of.cut(t, "end").trim().to_string(),
            format,
            constant,
            at: x.pos(21),
        });
    }
    b.module.outputs.push(out);
}

/// A fixed-form embedded SQL block: kept as one statement whose
/// factor 2 is the statement text, for the precompiler.
pub fn sql_block(b: &mut Builder<'_>, idx: &[usize]) {
    let mut text = String::new();
    let first = b.lines[idx[0]].no;
    for &i in idx {
        let l = &b.lines[i];
        // A line break per source line from the first.
        let want = l.no.saturating_sub(first) as usize;
        let breaks = want.saturating_sub(text.matches('\n').count());
        text.push_str(&"\n".repeat(breaks));
        let t = cut(&l.text, 8, 80);
        let up = t.trim().to_ascii_uppercase();
        let body = if up.starts_with("EXEC SQL") {
            t.trim()[8..].to_string()
        } else if up.starts_with("END-EXEC") {
            String::new()
        } else {
            t.trim_end().to_string()
        };
        if !text.is_empty() && !body.is_empty() && !text.ends_with('\n') {
            text.push(' ');
        }
        text.push_str(body.trim());
    }
    let at = b.lines[idx[0]].pos(7);
    b.stmt(Stmt {
        at,
        op: Op {
            code: "EXEC SQL".into(),
            f2: vec![Expr::Lit(Lit::Char(text), at)],
            ..Op::default()
        },
        ..Stmt::default()
    });
}

/// An indicator a resulting-indicator entry may name.
fn valid_indicator(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 2 {
        return false;
    }
    let (x, y) = (b[0], b[1]);
    match x {
        b'0'..=b'9' => y.is_ascii_digit() && s != "00",
        b'L' => y == b'R' || (b'1'..=b'9').contains(&y),
        b'H' => (b'1'..=b'9').contains(&y),
        b'O' => (b'A'..=b'G').contains(&y) || y == b'V',
        b'U' => (b'1'..=b'8').contains(&y),
        b'K' => y.is_ascii_uppercase() && y != b'O' && y != b'Z',
        b'R' => y == b'T',
        b'M' => y == b'R',
        _ => false,
    }
}
