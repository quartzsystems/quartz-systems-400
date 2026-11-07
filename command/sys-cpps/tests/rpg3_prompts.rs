// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTRPGPGM`'s and `CVTRPGSRC`'s prompts against pub400's captures
//! under `tools/conformance/golden/phase12/rpg3/` (V7R5M0), with and
//! without `F10`, every page. Rows 5 to 20 are compared: the prompt
//! texts and the values; the prompter's columns are its own.

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
        .join("../../tools/conformance/golden/phase12/rpg3")
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

/// The parameter rows of a prompt's pages (the screens after each key
/// set), in order, against the captures' rows in order: where a page
/// breaks is the prompter's own.
fn check_pages(names: &[&str], text: &str, keys: &[&[u8]]) {
    let mut src = source();
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            .filter(|r| {
                !r.is_empty() && !r.trim().starts_with('+') && r.trim() != "Additional Parameters"
            })
            .collect()
    };
    let mut ours: Vec<String> = Vec::new();
    for ks in keys {
        let r = {
            let mut env = Env::new(&mut src);
            env.setting.interactive = true;
            analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
        };
        let mut p = Prompter::new(&r);
        for k in *ks {
            let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
            let mut env = Env::new(&mut src);
            env.setting.interactive = true;
            assert_eq!(p.key(*k, &fields, &mut env), Step::Show, "{text}: key {k}");
        }
        for r in rows(&p.screen().rows) {
            if !ours.contains(&r) {
                ours.push(r);
            }
        }
    }
    let mut want: Vec<String> = Vec::new();
    for n in names {
        for r in rows(&golden_rows(n)) {
            if !want.contains(&r) {
                want.push(r);
            }
        }
    }
    let label = |r: &str| {
        r.chars()
            .take(34)
            .collect::<String>()
            .trim_end_matches(['.', ' ', ':'])
            .trim_start()
            .to_string()
    };
    let mut diffs = Vec::new();
    for (i, w) in want.iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let values: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| {
                !t.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
            .collect();
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !values.iter().all(|v| tail.contains(v)) {
            diffs.push(format!(
                "row {}:
  want {w:?}
  got  {got:?}",
                i + 1
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{text}:
{}",
        diffs.join(
            "
"
        )
    );
}

#[test]
fn crtrpgpgm() {
    check("prompt-crtrpgpgm", "?CRTRPGPGM", &[]);
    check_pages(
        &[
            "prompt-crtrpgpgm-f10",
            "prompt-crtrpgpgm-f10-2",
            "prompt-crtrpgpgm-f10-3",
        ],
        "?CRTRPGPGM",
        &[&[10], &[10, 8], &[10, 8, 8]],
    );
}

#[test]
fn cvtrpgsrc() {
    check("prompt-cvtrpgsrc", "?CVTRPGSRC", &[]);
    check("prompt-cvtrpgsrc-f10", "?CVTRPGSRC", &[0, 10]);
}
