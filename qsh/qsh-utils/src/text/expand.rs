// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `expand [-t tablist] [file...]`: tabs to spaces.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input, inputs};
use crate::common::{complain_sys, usage};

enum Tabs {
    Every(usize),
    At(Vec<usize>),
}

impl Tabs {
    fn parse(s: &str) -> Option<Self> {
        let nums: Vec<usize> = s
            .split([',', ' '])
            .filter(|x| !x.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        match nums.as_slice() {
            [] => None,
            [n] if *n > 0 => Some(Self::Every(*n)),
            [_] => None,
            list => {
                if list.windows(2).any(|w| w[0] >= w[1]) || list[0] == 0 {
                    return None;
                }
                Some(Self::At(list.to_vec()))
            }
        }
    }

    /// The next tab stop after column `col` (0-based), or `None` past
    /// the last fixed stop.
    fn next(&self, col: usize) -> Option<usize> {
        match self {
            Self::Every(n) => Some((col / n + 1) * n),
            Self::At(list) => list.iter().copied().find(|&t| t > col),
        }
    }
}

/// Run `expand`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "expand";
    let p = match getopt(&cx.argv[1..], "t:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let tabs = match p.value('t') {
        None => Tabs::Every(8),
        Some(s) => match Tabs::parse(s) {
            Some(t) => t,
            None => return usage(cx, name, &format!("The tab list {s} is not valid.")),
        },
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for f in inputs(&p.operands) {
        let fd = match input(cx, &f) {
            Ok(fd) => fd,
            Err(e) => {
                complain_sys(cx, name, &f, e);
                status = 1;
                continue;
            }
        };
        let mut rd = Lines::new(fd);
        while let Ok(Some(line)) = rd.next_line() {
            let mut o = Vec::with_capacity(line.len());
            let mut col = 0usize;
            for &b in &line {
                match b {
                    b'\t' => match tabs.next(col) {
                        Some(stop) => {
                            while col < stop {
                                o.push(b' ');
                                col += 1;
                            }
                        }
                        None => {
                            o.push(b' ');
                            col += 1;
                        }
                    },
                    b'\x08' => {
                        o.push(b);
                        col = col.saturating_sub(1);
                    }
                    b'\n' | b'\r' => {
                        o.push(b);
                        col = 0;
                    }
                    _ => {
                        o.push(b);
                        if b & 0xC0 != 0x80 {
                            col += 1;
                        }
                    }
                }
            }
            if !out.write(&o) {
                return out.finish(cx, name, status);
            }
        }
    }
    out.finish(cx, name, status)
}
