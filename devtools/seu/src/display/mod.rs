// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SEU's displays drawn on a screen model through the shipped display
//! files (`ui/dspf/sys/QSU*.dspf`) and read back from it. The job's
//! driver moves the model to the device; the tests compare it with the
//! oracle's screens. The fixed-form prompts are drawn from the column
//! tables onto the rows the edit display leaves them.

pub mod check;
mod keys;
mod screens;

use std::collections::BTreeMap;

use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, DisplayFile, NoRefs, Source, compile_dspf};
use tn5250::ds::attr;
use tn5250::ds::field::Ffw;
use tn5250::screen::ScreenModel;

use crate::session::{Input, RowInput, RowKind, View};

pub use keys::Legend;
pub use screens::{BrowseForm, Screens};

/// A shipped display file compiled.
///
/// # Panics
/// When the shipped source does not compile (a test keeps it so).
#[must_use]
pub fn compiled(text: &str) -> DisplayFile {
    let (src, _) = Source::from_text(text);
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    c.definition.expect("a shipped display file compiles")
}

/// An output buffer for a record: the named fields set, the rest blank.
#[must_use]
pub fn out_buffer(df: &DisplayFile, format: &str, values: &[(&str, &str)]) -> Vec<u8> {
    let Some(rec) = df.record(format) else {
        return Vec::new();
    };
    let mut out = vec![ccsid::SPACE; rec.layout.output.len as usize];
    for (name, value) in values {
        let Some((i, _)) = rec.field(name) else {
            continue;
        };
        let Some(bf) = rec.layout.output.field(i) else {
            continue;
        };
        let bytes = dspf::rt::input::padded(value, bf.len as usize);
        let at = bf.offset as usize;
        out[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    out
}

/// The separate indicator area with the indicators named on.
#[must_use]
pub fn indara(on: &[u8]) -> Vec<u8> {
    let mut a = vec![0xF0u8; 99];
    for n in on {
        if (1..=99).contains(n) {
            a[usize::from(*n) - 1] = 0xF1;
        }
    }
    a
}

/// Write a record of an open file.
pub fn put(rt: &mut Runtime, m: &mut ScreenModel, env: &Env, format: &str, values: &[(&str, &str)], on: &[u8]) {
    let out = out_buffer(rt.file(), format, values);
    let _ = rt.write(m, env, format, &out, &indara(on));
}

/// The text of the input field starting at a position.
#[must_use]
pub fn field_text(m: &ScreenModel, row: usize, col: usize) -> String {
    m.field_index_at(row, col)
        .and_then(|i| m.fields().get(i))
        .map(tn5250::screen::Field::text)
        .unwrap_or_default()
}

/// Text centred in a width.
fn centred(text: &str, width: usize) -> String {
    let n = text.chars().count().min(width);
    let pad = (width - n) / 2;
    format!("{}{text}", " ".repeat(pad))
}

/// The environment the tests and a job without one use.
#[must_use]
pub fn plain_env() -> Env {
    Env {
        messages: BTreeMap::new(),
        ..Env::default()
    }
}

/// Draw the edit or browse display.
pub fn show_session(s: &mut Screens, m: &mut ScreenModel, env: &Env, v: &View) {
    let rt = &mut s.seu;
    let title = centred(&v.header.title, 25);
    put(
        rt,
        m,
        env,
        "HEADER",
        &[
            ("COLS", v.header.columns.get(1..).unwrap_or("")),
            ("TITLE", &title),
            ("RIGHT", &v.header.right),
            ("CMD", &v.cmdline),
            ("MBR", &v.header.member),
            ("FMTLBL", &v.header.fmt_label),
            ("RULER", &v.header.fmt_ruler),
        ],
        &[],
    );
    let w = v.header.fmt_ruler.chars().count();
    for r in &v.rows {
        if r.kind == RowKind::Blank {
            continue;
        }
        let mut data: Vec<char> = format!("{:<w$}", r.data).chars().collect();
        if let Some((col, date)) = &r.date {
            let at = col - crate::session::DATA_COL;
            if data.len() < at {
                data.resize(at, ' ');
            }
            data.truncate(at);
            data.extend(date.chars());
        }
        let data: String = data.into_iter().take(crate::session::WINDOW).collect();
        let lineno = format!("{:02}", r.row - 1);
        let on: &[u8] = if r.kind == RowKind::Marker { &[50] } else { &[] };
        put(rt, m, env, "LINE", &[("LINENO", &lineno), ("SEQ", &r.seq), ("DATA", &data)], on);
    }
    if let Some(p) = &v.prompt {
        let lineno = format!("{:02}", p.top);
        put(
            rt,
            m,
            env,
            "PROMPT",
            &[("LINENO", &lineno), ("PTYPE", &format!("{:>2}", p.format)), ("PSEQ", &p.seq)],
            &[],
        );
        for (row, col, text) in &p.labels {
            m.put(*row, *col, text, attr::GREEN);
        }
        if let Some((row, col, text)) = &p.ruler {
            m.put(*row, *col, text, attr::GREEN);
        }
        for (row, col, len, value) in &p.fields {
            if *len > 0 {
                m.field(*row, *col, *len, attr::GREEN_UL, value, Ffw::default(), &[]);
            }
        }
    }
    let (k1, k2) = match v.keyset {
        Some(set) => keys::session(v.mode, set, v.prompt.is_some()),
        None => (Legend::default(), Legend::default()),
    };
    let copyright = v.copyright && v.message.is_none();
    put(
        rt,
        m,
        env,
        "KEYS",
        &[("KEYS1", &k1.text(2)), ("KEYS2", &k2.text(2))],
        if copyright { &[40] } else { &[] },
    );
    if let Some(msg) = &v.message {
        m.error_line(msg, attr::WHITE);
    }
    m.set_cursor(v.cursor.0, v.cursor.1);
}

/// What the operator typed on the edit or browse display.
#[must_use]
pub fn read_session(m: &ScreenModel, v: &View, cursor: (usize, usize)) -> Input {
    let w = v.header.fmt_ruler.chars().count();
    let rows = v
        .rows
        .iter()
        .filter(|r| r.kind != RowKind::Blank)
        .map(|r| RowInput {
            row: r.row,
            seq: field_text(m, r.row, 1),
            data: field_text(m, r.row, crate::session::DATA_COL).chars().take(w).collect(),
        })
        .collect();
    let prompt = v
        .prompt
        .as_ref()
        .map(|p| {
            p.fields
                .iter()
                .map(|(row, col, len, _)| {
                    let t = field_text(m, *row, *col);
                    format!("{t:<len$}")
                })
                .collect()
        })
        .unwrap_or_default();
    Input {
        cmdline: field_text(m, 2, crate::session::DATA_COL),
        rows,
        cursor,
        prompt,
    }
}
