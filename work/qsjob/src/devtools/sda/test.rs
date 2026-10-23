// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 3, Test Display File: the record's output values and option
//! indicators are typed on Set Test Output Data, the record is written
//! and read through `dspf::rt` like a program would, and what came
//! back shows on Display Test Input Data.

use std::collections::BTreeMap;

use dspf::rt::{Problem, Runtime};
use sda::screen::{entry, test as tscreen};
use stg::Client;
use tn5250::ds::aid;
use tn5250::screen::Read;

use super::ui::{self, Key};
use super::{Flow, Session};
use crate::console::host::Host;
use crate::display::dds::{env_of, in_text, out_buffer, system_name};
use crate::display::surface::Surface;

/// Test Display File until F3 or F12.
pub fn start(host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let mut message: Option<String> = None;
    loop {
        let mut vals = BTreeMap::new();
        vals.insert("FILE".to_string(), s.test.0.clone());
        vals.insert("LIB".to_string(), s.test.1.clone());
        vals.insert("RECORD".to_string(), s.test.2.clone());
        vals.insert("MORE".to_string(), s.test.3.clone());
        let screen = entry::test_dspf(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        match a.key() {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        s.test = (a.up("FILE"), a.up("LIB"), a.up("RECORD"), a.up("MORE"));
        let (file, lib, record, more) = s.test.clone();
        let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
        let opened = host
            .link
            .borrow_mut()
            .with(|l| l.dsp_open(file.clone(), lib.clone(), "*DFTACTGRP".into()));
        let df = match opened.map(|(p, _)| dspf::decode(&p)) {
            Ok(Ok(df)) => df,
            Ok(Err(_)) => {
                message = Some(format!("Display file {file} cannot be read."));
                continue;
            }
            Err(e) => {
                message = Some(e);
                continue;
            }
        };
        if record.is_empty() || df.record(&record).is_none() {
            message = Some(format!("Type a record of display file {file}."));
            continue;
        }
        if !more.is_empty() && df.record(&more).is_none() {
            message = Some(format!("Record {more} is not in display file {file}."));
            continue;
        }
        match test(host, surface, &file, &df, &record, &more) {
            Flow::Gone => return Flow::Gone,
            Flow::Exit => return Flow::Exit,
            Flow::Back => {}
        }
    }
}

/// Indicator numbers typed as a list.
fn indicators(text: &str) -> Result<Vec<u8>, String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(|w| {
            w.parse::<u8>()
                .ok()
                .filter(|n| (1..=99).contains(n))
                .ok_or_else(|| format!("Indicator {w} is not 01 to 99."))
        })
        .collect()
}

/// The name of the key that ended a read.
fn key_name(a: u8) -> String {
    match a {
        aid::ENTER | aid::AUTO_ENTER => "Enter".into(),
        aid::ROLL_UP => "Page Down".into(),
        aid::ROLL_DOWN => "Page Up".into(),
        aid::HELP => "Help".into(),
        aid::PRINT => "Print".into(),
        other => aid::function_of(other).map_or_else(|| format!("X'{other:02X}'"), |n| format!("F{n}")),
    }
}

/// Set the output, show the record, read it and show what came back.
#[allow(clippy::too_many_lines)] // the test's three steps
fn test(host: &Host, surface: &mut Surface, file: &str, df: &dspf::DisplayFile, record: &str, more: &str) -> Flow {
    let rec = df.record(record).cloned().expect("the record was checked");
    let outputs: Vec<(String, usize)> = rec
        .layout
        .output
        .fields
        .iter()
        .filter_map(|bf| {
            let f = &rec.fields[bf.field];
            f.name.clone().map(|n| (n, f.screen_len.max(1) as usize))
        })
        .collect();
    let mut values: Vec<String> = vec![String::new(); outputs.len()];
    let mut inds = String::new();
    let mut message: Option<String> = None;
    let mut top = 0;
    let sysname = system_name(host);
    loop {
        let shown: Vec<tscreen::TestField> = outputs
            .iter()
            .zip(&values)
            .map(|((n, l), v)| (n.clone(), v.clone(), *l))
            .collect();
        let screen = tscreen::set_output(file, record, &inds, &shown, top, message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        for (i, value) in values.iter_mut().enumerate() {
            if let Some(v) = a.cells.get(&(i, "VALUE".to_string())) {
                value.clone_from(v);
            }
        }
        inds = a.raw("INDS");
        match a.key() {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::PageDown if top + tscreen::PAGE < outputs.len() => {
                top += tscreen::PAGE;
                continue;
            }
            Key::PageUp => {
                top = top.saturating_sub(tscreen::PAGE);
                continue;
            }
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let on = match indicators(&inds) {
            Ok(v) => v,
            Err(e) => {
                message = Some(e);
                continue;
            }
        };
        // The buffers: values, and the indicators where the format keeps them.
        let pairs: Vec<(&str, &str)> = outputs
            .iter()
            .zip(&values)
            .map(|((n, _), v)| (n.as_str(), v.as_str()))
            .collect();
        let mut out = out_buffer(&rec, &pairs);
        for (i, n) in rec.layout.output.indicators.iter().enumerate() {
            if on.contains(n) {
                out[i] = 0xF1;
            }
        }
        let mut indara = vec![0xF0u8; 99];
        if df.file.indara {
            for n in &on {
                indara[usize::from(*n) - 1] = 0xF1;
            }
        }
        let env = env_of(host, &sysname);
        let mut rt = Runtime::new(df.clone(), surface.wide());
        let wrote = surface.with_model(|m| {
            if !more.is_empty()
                && let Some(mr) = df.record(more)
            {
                let mo = out_buffer(mr, &[]);
                rt.write(m, &env, more, &mo, &indara)?;
            }
            rt.write(m, &env, record, &out, &indara)
        });
        if let Err(e) = wrote {
            message = Some(format!("Record {record} cannot be shown: {e}"));
            continue;
        }
        let input = loop {
            surface.flush(Read::MdtFields);
            let Some((aid_byte, cursor)) = surface.wait() else {
                return Flow::Gone;
            };
            match surface.with_model(|m| rt.input(m, &env, record, aid_byte, cursor)) {
                Ok(i) => break Some(i),
                Err(Problem::NotShown(_)) => break None,
                Err(p) => surface.with_model(|m| rt.show_problem(m, &p)),
            }
        };
        let Some(input) = input else {
            message = Some(format!("Record {record} was not shown."));
            continue;
        };
        let back_on: Vec<u8> = if df.file.indara {
            (1..=99u8)
                .filter(|n| input.indicators.get(usize::from(*n) - 1) == Some(&0xF1))
                .collect()
        } else {
            rec.layout
                .input
                .indicators
                .iter()
                .enumerate()
                .filter(|(i, _)| input.record.get(*i) == Some(&0xF1))
                .map(|(_, n)| *n)
                .collect()
        };
        let got: Vec<tscreen::TestField> = rec
            .layout
            .input
            .fields
            .iter()
            .filter_map(|bf| rec.fields[bf.field].name.clone())
            .map(|n| {
                let v = in_text(&rec, &input.record, &n).unwrap_or_default();
                (n, v, 0)
            })
            .collect();
        let inds_text = back_on
            .iter()
            .map(|n| format!("{n:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut itop = 0;
        loop {
            let screen = tscreen::show_input(file, record, &key_name(input.aid), &inds_text, &got, itop, None);
            let Some(a) = ui::show(surface, &screen) else {
                return Flow::Gone;
            };
            match a.key() {
                Key::F(3) => return Flow::Exit,
                Key::PageDown if itop + tscreen::PAGE < got.len() => itop += tscreen::PAGE,
                Key::PageUp => itop = itop.saturating_sub(tscreen::PAGE),
                _ => break,
            }
        }
    }
}
