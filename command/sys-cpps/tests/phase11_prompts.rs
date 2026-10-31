// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 11 prompts against pub400's captures under
//! `tools/conformance/golden/phase11/` (V7R5M0), as an interface: the
//! parameters by keyword in display order across every page, where
//! `Additional Parameters` starts, each field's length and default,
//! the qualifier and element fields under a parameter, the title's
//! `(CMD)` and the function keys. The words are not compared (labels
//! are `media.rs`'s business, help text is ours), nor the columns:
//! pub400 shows keywords (field at column 50), our default view does
//! not (field at column 37). The method is in `COVERAGE.md` there.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Screen, Step};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase11")
}

/// Fields this long or longer are "long": pub400 and we cut them at
/// different widths.
const LONG: usize = 40;

/// One parameter as shown: its keyword, its field and the fields of
/// its qualifiers or elements.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    /// `Additional Parameters`.
    Divider,
    /// A parameter.
    Parm {
        kwd: String,
        field: Option<(usize, String)>,
        subs: Vec<(usize, String)>,
    },
}

fn read(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| format!("{l:<80}"))
        .collect()
}

/// A golden page: its text and attribute grids.
struct Page {
    text: Vec<String>,
    attr: Vec<String>,
}

impl Page {
    fn load(dir: &str, name: &str) -> Self {
        let d = golden_dir().join(dir);
        Self {
            text: read(&d.join(format!("{name}.txt"))),
            attr: read(&d.join(format!("{name}.attr"))),
        }
    }

    fn cell(grid: &[String], i: usize) -> char {
        grid.get(i / 80).and_then(|r| r.chars().nth(i % 80)).unwrap_or(' ')
    }

    /// The input fields starting on `row` from column `from` (both
    /// 0-based, as is the column returned):
    /// underlined, displayed runs after an attribute byte, as (column,
    /// length, value). A run may wrap onto the next row.
    fn fields(&self, row: usize, from: usize) -> Vec<(usize, usize, String)> {
        // `0`.. is attribute 0x20..: underlined (0x04) and not
        // nondisplay (0x07).
        let input = |c: char| {
            let a = u32::from(c).checked_sub(u32::from('0')).map_or(0, |d| d + 0x20);
            (0x20..=0x3f).contains(&a) && a & 0x04 != 0 && a & 0x07 != 0x07
        };
        let mut out = Vec::new();
        let mut c = from;
        while c < 80 {
            let i = row * 80 + c;
            if Self::cell(&self.attr, i) == '#' && input(Self::cell(&self.attr, i + 1)) {
                let mut n = 0;
                let mut v = String::new();
                while input(Self::cell(&self.attr, i + 1 + n)) {
                    v.push(Self::cell(&self.text, i + 1 + n));
                    n += 1;
                }
                out.push((c + 1, n, v.trim().to_string()));
                c += n + 1;
            } else {
                c += 1;
            }
        }
        out
    }

    /// The parameters of rows 5 to 20. A keyword starts in column 35;
    /// a labelled row without one is a qualifier or element of the
    /// parameter above.
    fn entries(&self, out: &mut Vec<Entry>) {
        for r in 4..20 {
            let t = &self.text[r];
            if t.trim() == "Additional Parameters" {
                out.push(Entry::Divider);
                continue;
            }
            if t.trim_start().starts_with("+ for more values") {
                continue;
            }
            let label = t.get(1..33).unwrap_or("").trim();
            let kwd: String = t.get(34..48).unwrap_or("").split_whitespace().next().unwrap_or("").to_string();
            let is_kwd = t.get(33..34) == Some(" ")
                && !kwd.is_empty()
                && kwd.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            // Value fields start in column 50 (qualifiers 52); the
            // one-position field in column 48 is a group list's `+`.
            let field = self.fields(r, 44).into_iter().find(|f| f.0 >= 49).map(|(_, n, v)| (n, v));
            if is_kwd {
                out.push(Entry::Parm { kwd, field, subs: Vec::new() });
            } else if !label.is_empty()
                && let Some(f) = field
                && let Some(Entry::Parm { subs, .. }) = out.last_mut()
            {
                subs.push(f);
            }
        }
    }
}

/// The parameters of our screen, in the same terms.
/// `state`: a heading waiting for its parts and the parameter of the
/// last field, carried from page to page.
fn our_entries(p: &Prompter, s: &Screen, state: &mut (bool, Option<usize>), out: &mut Vec<Entry>) {
    let (pending_head, current) = state;
    for r in 5..=20u8 {
        let t = s.rows.get(usize::from(r) - 1).cloned().unwrap_or_default();
        if t.trim() == "Additional Parameters" {
            out.push(Entry::Divider);
            *current = None;
            continue;
        }
        let f = s
            .fields
            .iter()
            .find(|f| f.row == r && f.path.as_ref().is_some_and(|p| p.steps.last() != Some(&usize::MAX)));
        let Some(f) = f else {
            if t.trim_start().starts_with("+ for more values") {
                continue;
            }
            if !t.trim().is_empty() {
                *pending_head = true;
            }
            continue;
        };
        let path = f.path.as_ref().expect("a path");
        let fv = (usize::from(f.len), f.value.trim().to_string());
        let label = t.get(1..33).unwrap_or("").trim().to_string();
        if *current == Some(path.parm) {
            if !label.is_empty()
                && let Some(Entry::Parm { subs, .. }) = out.last_mut()
            {
                subs.push(fv);
            }
            *pending_head = false;
            continue;
        }
        let kwd = p.def.parms[path.parm].kwd.clone();
        *current = Some(path.parm);
        if *pending_head {
            out.push(Entry::Parm { kwd, field: None, subs: vec![fv] });
        } else {
            out.push(Entry::Parm { kwd, field: Some(fv), subs: Vec::new() });
        }
        *pending_head = false;
    }
}

/// `F3`, `F4`, ... on the key rows.
fn keys(rows: &[String]) -> BTreeSet<String> {
    rows.iter()
        .flat_map(|r| r.split_whitespace())
        .filter_map(|w| w.split_once('=').map(|(k, _)| k.to_string()))
        .filter(|k| k.starts_with('F') && k[1..].chars().all(|c| c.is_ascii_digit()))
        .collect()
}

/// The golden pages of `base`: `base`, then `base-2`, `base-3`, ...
fn pages(dir: &str, base: &str) -> Vec<Page> {
    let mut v = vec![Page::load(dir, base)];
    for n in 2.. {
        let name = format!("{base}-{n}");
        if !golden_dir().join(dir).join(format!("{name}.txt")).exists() {
            break;
        }
        v.push(Page::load(dir, &name));
    }
    v
}

/// A parameter split by a page break is one parameter.
fn merged(v: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in v {
        if let (Some(Entry::Parm { kwd: a, subs, .. }), Entry::Parm { kwd: b, field, subs: more }) = (out.last_mut(), &e)
            && a == b
        {
            subs.extend(field.clone());
            subs.extend(more.iter().cloned());
            continue;
        }
        out.push(e);
    }
    out
}

fn same_field(want: &(usize, String), got: &(usize, String)) -> bool {
    let len = want.0 == got.0 || (want.0 >= LONG && got.0 >= LONG);
    len && want.1.eq_ignore_ascii_case(&got.1)
}


/// One golden (with its following pages) against our prompt.
struct Case {
    /// The golden's name.
    golden: &'static str,
    /// Its folder under `phase11/`.
    dir: &'static str,
    /// The command prompted.
    text: &'static str,
    /// The keys pressed before the comparison.
    keys: &'static [u8],
    /// Parameters pub400 shows that we do not define.
    ibm_only: &'static [&'static str],
    /// Parameters we show that pub400 does not.
    ours_only: &'static [&'static str],
    /// Parameters whose field or parts differ for a recorded reason
    /// (`COVERAGE.md`); `keys` for the function key line.
    known: &'static [&'static str],
}

const fn case(golden: &'static str, text: &'static str, keys: &'static [u8]) -> Case {
    Case { golden, dir: "prompts", text, keys, ibm_only: &[], ours_only: &[], known: &[] }
}

/// The differences of one case, as text.
#[allow(clippy::many_single_char_names, clippy::too_many_lines)] // one comparison, top to bottom
fn differences(c: &Case) -> Vec<String> {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        match analyze(&mut env, c.text) {
            Ok(r) => r,
            Err(e) => return vec![format!("{}: {e}", c.text)],
        }
    };
    let mut p = Prompter::new(&r);
    for k in c.keys {
        let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        let st = p.key(*k, &fields, &mut env);
        if st != Step::Show {
            return vec![format!("key {k}: {st:?}")];
        }
    }
    let gold = pages(c.dir, c.golden);
    let mut diffs = Vec::new();
    let first = p.screen();
    // The title's command name and the keys of the first page.
    let name = format!("({})", p.name);
    if !gold[0].text[0].contains(&name) || !first.rows[0].contains(&name) {
        diffs.push(format!("title: want {:?} got {:?}", gold[0].text[0].trim(), first.rows[0].trim()));
    }
    let want_keys = keys(&gold[0].text[21..23]);
    let got_keys = keys(&first.rows[21..24]);
    if want_keys != got_keys && !c.known.contains(&"keys") {
        diffs.push(format!("keys: want {want_keys:?} got {got_keys:?}"));
    }
    // Every page of the capture; ours to Bottom when the capture ends
    // there, else as many pages.
    let mut want = Vec::new();
    for g in &gold {
        g.entries(&mut want);
    }
    let complete = gold.last().is_some_and(|g| g.text[20].trim() == "Bottom");
    let mut got = Vec::new();
    let mut state = (false, None);
    for n in 0..30 {
        let s = p.screen();
        our_entries(&p, &s, &mut state, &mut got);
        if s.rows.iter().any(|r| r.trim() == "Bottom") || (!complete && n + 1 >= gold.len()) {
            break;
        }
        let fields: Vec<String> = s.fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        p.key(8, &fields, &mut env);
    }
    let (want, got) = (merged(want), merged(got));
    let kw = |v: &[Entry], skip: &[&str]| -> Vec<String> {
        v.iter()
            .filter_map(|e| match e {
                Entry::Divider => Some("|".to_string()),
                Entry::Parm { kwd, .. } => (!skip.contains(&kwd.as_str())).then(|| kwd.clone()),
            })
            .collect()
    };
    let all_want = kw(&want, &[]);
    for k in c.ibm_only {
        if p.def.parms.iter().any(|q| q.kwd == *k) || !all_want.iter().any(|w| w == k) {
            diffs.push(format!("{k} is not pub400's alone"));
        }
    }
    for k in c.ours_only {
        if all_want.iter().any(|w| w == k) {
            diffs.push(format!("{k} is pub400's too"));
        }
    }
    let (mut wk, mut gk) = (kw(&want, c.ibm_only), kw(&got, c.ours_only));
    // A divider with nothing after it is no divider.
    for v in [&mut wk, &mut gk] {
        if v.last().is_some_and(|k| k == "|") {
            v.pop();
        }
    }
    let gk_cmp: &[String] = if complete { &gk } else { &gk[..gk.len().min(wk.len())] };
    if wk != gk_cmp {
        diffs.push(format!("order:\n    want {}\n    got  {}", wk.join(" "), gk.join(" ")));
    }
    for w in &want {
        let Entry::Parm { kwd, field, subs } = w else {
            continue;
        };
        if c.known.contains(&kwd.as_str()) {
            continue;
        }
        let Some(Entry::Parm { field: gf, subs: gs, .. }) =
            got.iter().find(|g| matches!(g, Entry::Parm { kwd: k, .. } if k == kwd))
        else {
            continue;
        };
        match (field, gf) {
            (Some(a), Some(b)) if !same_field(a, b) => diffs.push(format!("{kwd}: want {a:?} got {b:?}")),
            (Some(a), None) => diffs.push(format!("{kwd}: want {a:?} got a heading")),
            (None, Some(b)) => diffs.push(format!("{kwd}: want a heading got {b:?}")),
            _ => {}
        }
        if subs.len() != gs.len() || subs.iter().zip(gs).any(|(a, b)| !same_field(a, b)) {
            diffs.push(format!("{kwd} parts: want {subs:?} got {gs:?}"));
        }
    }
    diffs
}

fn check(cases: &[Case]) {
    let mut bad = Vec::new();
    for c in cases {
        let d = differences(c);
        if !d.is_empty() {
            bad.push(format!("{}:\n  {}", c.golden, d.join("\n  ")));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

/// pub400's save parameters we do not define (`COVERAGE.md`).
const SAV_IBM_ONLY: &[&str] = &["SYNCID", "PVTAUT", "STG", "SELECT", "CMDUSRSPC", "SORT"];

/// Recorded differences of the save commands: `SAVACTMSGQ`'s library
/// (pub400 shows `*LIBL` beside its `*NONE`) and the parts of lists of
/// groups (pub400 fills element defaults after Enter).
const SAV_KNOWN: &[&str] = &["SAVACTMSGQ", "OMITOBJ", "FILEMBR"];

#[test]
fn save_prompts() {
    check(&[
        case("prompt-savlib", "?SAVLIB", &[]),
        Case { ibm_only: SAV_IBM_ONLY, known: SAV_KNOWN, ..case("prompt-savlib-f10", "?SAVLIB", &[10]) },
        case("prompt-savobj", "?SAVOBJ", &[]),
        Case {
            ibm_only: &["SYNCID", "STG", "SELECT", "CMDUSRSPC"],
            known: SAV_KNOWN,
            ..case("prompt-savobj-f10", "?SAVOBJ", &[10])
        },
        // pub400 shows no default for OBJ.
        Case { known: &["OBJ"], ..case("prompt-savchgobj", "?SAVCHGOBJ", &[]) },
        Case {
            ibm_only: &["SYNCID", "SELECT", "CMDUSRSPC", "STRLIB"],
            ours_only: &["FILEMBR"],
            known: &["SAVACTMSGQ", "OMITOBJ", "OBJ"],
            ..case("prompt-savchgobj-f10", "?SAVCHGOBJ", &[10])
        },
        // The pattern's default: pub400 shows '*', we show *.
        Case { known: &["PATTERN"], ..case("prompt-sav", "?SAV", &[]) },
        Case {
            ibm_only: &["EXPDATE", "USEOPTBLK"],
            // pub400 shows these only on conditions it alone knows.
            ours_only: &["SYSTEM", "PRECHK", "TGTRLS", "UPDHST", "CLEAR", "DTACPR"],
            known: &["PATTERN"],
            ..case("prompt-sav-f10", "?SAV", &[10])
        },
        case("prompt-savsecdta", "?SAVSECDTA", &[]),
        Case {
            ibm_only: &["PRECHK"],
            ours_only: &["LABEL", "MEDDFN"],
            ..case("prompt-savsecdta-f10", "?SAVSECDTA", &[10])
        },
        case("prompt-savcfg", "?SAVCFG", &[]),
        Case {
            ibm_only: &["PRECHK"],
            ours_only: &["LABEL", "MEDDFN"],
            ..case("prompt-savcfg-f10", "?SAVCFG", &[10])
        },
    ]);
}

#[test]
fn save_file_prompts() {
    check(&[
        case("prompt-crtsavf", "?CRTSAVF", &[]),
        case("prompt-crtsavf-f10", "?CRTSAVF", &[10]),
        // pub400's CHGSAVF opens on its key parameter alone.
        Case { ours_only: &["MAXRCDS", "TEXT"], ..case("prompt-chgsavf", "?CHGSAVF", &[]) },
        case("prompt-chgsavf-f10", "?CHGSAVF", &[10]),
        case("prompt-clrsavf", "?CLRSAVF", &[]),
        case("prompt-dspsavf", "?DSPSAVF", &[]),
    ]);
}
