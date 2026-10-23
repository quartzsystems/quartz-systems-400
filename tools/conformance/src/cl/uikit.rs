// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM APIs in the CL kit: the kit's panel groups served as if
//! `CRTPNLGRP` had made them, and a scripted display that answers each
//! screen with the case's next step and keeps what it showed.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::Path;
use std::rc::Rc;

use mi::Job;
use sysapi::uiapi::{SharedUimHost, UimHost};
use uim::PanelGroup;
use uim::engine::{Input, Terminal};
use uim::model::Key;
use uim::render::{Screen, Target};

use super::Case;

/// The display the kit's job shows panels on.
#[derive(Default)]
pub struct KitDisplay {
    steps: VecDeque<String>,
    shown: Vec<Screen>,
}

struct Term(Rc<RefCell<KitDisplay>>);

impl Terminal for Term {
    fn wide(&mut self) -> bool {
        false
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        let mut d = self.0.borrow_mut();
        d.shown.push(screen.clone());
        let mut values: Vec<String> = screen.fields.iter().map(|f| f.value.clone()).collect();
        // Options typed (`5@1`) go with the key that follows them.
        loop {
            let step = d.steps.pop_front()?;
            if let Some((opt, entry)) = step.split_once('@') {
                let n: usize = entry.parse().unwrap_or(0);
                for (f, v) in screen.fields.iter().zip(values.iter_mut()) {
                    if matches!(&f.target, Target::ListOption { row_index, .. } if *row_index == n)
                    {
                        *v = opt.to_string();
                    }
                }
                continue;
            }
            let key = match step.as_str() {
                "enter" => Key::Enter,
                "pgdn" => Key::PageDown,
                "pgup" => Key::PageUp,
                "help" => Key::Help,
                other => other
                    .strip_prefix('f')
                    .and_then(|n| n.parse().ok())
                    .map_or(Key::Enter, Key::F),
            };
            return Some(Input {
                key: Some(key),
                cursor: (1, 1),
                values,
            });
        }
    }
}

struct KitHost {
    groups: Vec<(String, Rc<PanelGroup>)>,
    display: Rc<RefCell<KitDisplay>>,
}

impl UimHost for KitHost {
    fn panel_group(&mut self, _lib: &str, name: &str) -> Result<(Rc<PanelGroup>, String), String> {
        self.groups
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, g)| (Rc::clone(g), "QGPL".to_string()))
            .ok_or_else(|| format!("CPF9801 {name}"))
    }

    fn terminal(&mut self) -> Option<Box<dyn Terminal>> {
        Some(Box::new(Term(Rc::clone(&self.display))))
    }
}

/// Install the UIM APIs over the case's panel groups and display.
pub fn install(job: &mut Job, case: &Case) -> Rc<RefCell<KitDisplay>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("cl")
        .join("pnlgrp");
    let mut groups = Vec::new();
    for name in &case.pnlgrps {
        let path = dir.join(format!("{name}.pnlgrp"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        let c = uim::compile(
            &text,
            &uim::NoImbeds,
            &uim::NoImports,
            &uim::CompileOpts::default(),
        );
        let ok = c.ok();
        if let Some(g) = c.group.filter(|_| ok) {
            groups.push((name.clone(), Rc::new(g)));
        }
    }
    let display = Rc::new(RefCell::new(KitDisplay {
        steps: case.display.iter().cloned().collect(),
        shown: Vec::new(),
    }));
    let host: SharedUimHost = Rc::new(RefCell::new(KitHost {
        groups,
        display: Rc::clone(&display),
    }));
    sysapi::uiapi::install(job, &host);
    display
}

/// The screens shown, their rows that hold something.
#[must_use]
pub fn screens(d: &Rc<RefCell<KitDisplay>>) -> String {
    let mut out = String::new();
    for (i, s) in d.borrow().shown.iter().enumerate() {
        let _ = writeln!(out, "screen {}:", i + 1);
        for (r, row) in s.text_rows().iter().enumerate() {
            if !row.trim().is_empty() {
                let _ = writeln!(out, "  {:2} |{}|", r + 1, row.trim_end());
            }
        }
    }
    out
}
