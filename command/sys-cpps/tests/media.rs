// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media commands' prompts against pub400's captures under
//! `tools/conformance/golden/phase11/prompts/` (V7R5M0): the tape,
//! optical and media library device descriptions, the image catalog
//! commands and the tape and optical commands, with and without `F10`.
//! Rows 5 to 20 are compared: the prompt texts and the values; the
//! prompter's columns are its own.

use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_rows(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase11/prompts")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The prompt of `text` after `keys`, against golden `name`, row by row.
fn check(name: &str, text: &str, keys: &[u8]) {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    let mut p = Prompter::new(&r);
    let mut before = p.screen().rows;
    for k in keys {
        before = p.screen().rows;
        let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        assert_eq!(p.key(*k, &fields, &mut env), Step::Show, "{name}: key {k}");
    }
    let s = p.screen();
    let want = golden_rows(name);
    let mut diffs = Vec::new();
    // The parameter rows: each row's prompt text (to column 34, dot
    // leaders aside) must match, and every value the capture shows after
    // the keyword must be on our row. The title, the instruction, the
    // function key rows and the columns are the prompter's own layout.
    let label = |r: &str| {
        if r.trim() == "Additional Parameters" {
            return r.trim().to_string();
        }
        r.chars()
            .take(34)
            .collect::<String>()
            .trim_end_matches(['.', ' ', ':'])
            .trim_start()
            .to_string()
    };
    // Blank rows and "+ for more values" aside: the prompter spaces
    // "Additional Parameters" and places the list marker its own way.
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            .filter(|r| !r.is_empty() && !r.trim().starts_with('+'))
            .collect()
    };
    let mut ours = rows(&s.rows);
    // A capture that starts at "Additional Parameters" is compared with
    // our parameters from that title on, across the two pages: where the
    // prompter breaks the page is its own.
    let title = |r: &String| r.trim() == "Additional Parameters";
    if rows(&want).first().is_some_and(title) {
        let mut both = rows(&before);
        both.extend(ours);
        ours = both.into_iter().skip_while(|r| !title(r)).collect();
    }
    for (i, w) in rows(&want).iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let values: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| {
                *t != ">"
                    && !t
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
            .collect();
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !values.iter().all(|v| tail.contains(v)) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{name}:\n{}", diffs.join("\n"));
}

#[test]
fn device_descriptions() {
    check("prompt-crtdevtap", "?CRTDEVTAP", &[]);
    check("prompt-crtdevtap-f10", "?CRTDEVTAP", &[0, 10]);
    check("prompt-crtdevopt", "?CRTDEVOPT", &[]);
    check("prompt-crtdevopt-f10", "?CRTDEVOPT", &[0, 10]);
    check("prompt-crtdevmlb", "?CRTDEVMLB", &[]);
    check("prompt-crtdevmlb-f10", "?CRTDEVMLB", &[0, 10]);
}

#[test]
fn image_catalogs() {
    check("prompt-crtimgclg", "?CRTIMGCLG", &[]);
    check("prompt-crtimgclg-f10", "?CRTIMGCLG", &[10]);
    check("prompt-addimgclge", "?ADDIMGCLGE", &[]);
    check("prompt-addimgclge-f10", "?ADDIMGCLGE", &[0, 10]);
    check("prompt-lodimgclg", "?LODIMGCLG", &[]);
    check("prompt-vfyimgclg", "?VFYIMGCLG", &[]);
    check("prompt-wrkimgclg", "?WRKIMGCLG", &[]);
}

#[test]
fn tape() {
    check("prompt-inztap", "?INZTAP", &[]);
    check("prompt-dsptap", "?DSPTAP", &[]);
    check("prompt-chktap", "?CHKTAP", &[]);
    check("prompt-duptap", "?DUPTAP", &[]);
}

#[test]
fn optical() {
    check("prompt-inzopt", "?INZOPT", &[]);
    check("prompt-inzopt-f10", "?INZOPT", &[0, 10]);
    check("prompt-dspopt", "?DSPOPT", &[]);
    check("prompt-dupopt", "?DUPOPT", &[]);
    check("prompt-dupopt-f10", "?DUPOPT", &[10]);
}

/// `ADDOPTCTG` against the 7.1 oracle's prompt (pub400 has no optical
/// device to add to).
#[test]
fn add_optical_cartridge() {
    check("prompt-addoptctg", "?ADDOPTCTG", &[]);
}

/// `WRKOPTVOL`'s option 1 on the add entry runs a command the analyzer
/// knows, whose prompt is `ADDOPTCTG`'s (`audit/displays/
/// wrkoptvol-add-prompt71`, the oracle's prompt after option 1).
#[test]
fn wrkoptvol_option_1_prompts_addoptctg() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/uim/pnlgrp/QSMEDIA.pnlgrp");
    let src = std::fs::read_to_string(&p).unwrap();
    let panel = src
        .split(":PANEL NAME=WRKOPTVOL ")
        .nth(1)
        .expect("WRKOPTVOL panel");
    let line = panel
        .lines()
        .find(|l| l.starts_with(":LISTACT OPTION=1 "))
        .expect("option 1");
    let cmd = line
        .split("ENTER='CMD ")
        .nth(1)
        .and_then(|s| s.split('\'').next())
        .expect("its command");
    assert_eq!(cmd, "?ADDOPTCTG");
    let mut s = source();
    let r = {
        let mut env = Env::new(&mut s);
        env.setting.interactive = true;
        analyze(&mut env, cmd).unwrap_or_else(|e| panic!("{cmd}: {e}"))
    };
    let got = Prompter::new(&r).screen().rows;
    let want = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/audit/displays/wrkoptvol-add-prompt71.txt");
    let want = std::fs::read_to_string(&want)
        .unwrap()
        .replace("\r\n", "\n");
    let want: Vec<&str> = want.lines().collect();
    assert_eq!(got[0].trim(), want[0].trim(), "the title");
    check("prompt-addoptctg", cmd, &[]);
}

#[test]
fn tape_copies() {
    check("prompt-cpytotap", "?CPYTOTAP", &[]);
    check("prompt-cpytotap-f10", "?CPYTOTAP", &[0, 10]);
    check("prompt-cpytotap-f10-2", "?CPYTOTAP", &[0, 10, 8]);
    check("prompt-cpyfrmtap", "?CPYFRMTAP", &[]);
    check("prompt-cpyfrmtap-f10", "?CPYFRMTAP", &[0, 10]);
    check("prompt-cpyfrmtap-f10-2", "?CPYFRMTAP", &[0, 10, 8]);
}
