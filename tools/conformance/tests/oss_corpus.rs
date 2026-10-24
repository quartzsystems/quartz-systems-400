// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The open-source corpus: RPG projects cloned into
//! `tools/conformance/oss/src/` (gitignored; see `docs/PHASE9.md` for
//! the list), every member compiled (as a module when it is NOMAIN,
//! else as a program), unmodified, with
//! `/COPY` and `/INCLUDE` found by name anywhere in its project. Members
//! may need files or programs the projects do not hold, so a member that
//! is not created is counted, not a failure; the compiler failing
//! (a panic, or IR it cannot verify: `RNF0001`) is. Prints the count per
//! project and the most common message IDs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use conformance::rpg;
use rpg_oracle::case::Case;
use rpgfront::{Includes, Member};

const MEMBER: &[&str] = &["rpgle", "sqlrpgle"];
const BROUGHT: &[&str] = &["rpgle", "sqlrpgle", "rpgleinc", "rpginc", "inc", "rpgleref", "cpy"];

fn ext(p: &Path) -> String {
    p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// A project's members by file name and by stem, upper case; by stem
/// an include member (`.rpgleinc`...) wins over a program source.
struct ByStem {
    files: HashMap<String, PathBuf>,
    stems: HashMap<String, PathBuf>,
}

impl ByStem {
    fn new(paths: &[PathBuf]) -> Self {
        let mut files = HashMap::new();
        let mut stems: HashMap<String, PathBuf> = HashMap::new();
        for p in paths.iter().filter(|p| BROUGHT.contains(&ext(p).as_str())) {
            let Some(base) = p.file_name().map(|n| n.to_string_lossy().to_ascii_uppercase()) else { continue };
            files.insert(base.clone(), p.clone());
            let stem = base.split('.').next().unwrap_or(&base).to_string();
            let include = !MEMBER.contains(&ext(p).as_str());
            let keep = stems.get(&stem).is_some_and(|q| !MEMBER.contains(&ext(q).as_str()) && !include);
            if !keep {
                stems.insert(stem, p.clone());
            }
        }
        Self { files, stems }
    }
}

impl Includes for ByStem {
    fn member(&mut self, _lib: &str, _file: &str, name: &str) -> Option<Member> {
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_uppercase();
        let stem = base.split('.').next().unwrap_or(&base).to_string();
        let p = self.files.get(&base).or_else(|| self.stems.get(&stem))?;
        let text = String::from_utf8_lossy(&std::fs::read(p).ok()?).to_string();
        Some(Member::from_text("OSS", "QRPGLESRC", &stem, &text))
    }
}

#[test]
fn open_source_members_compile_without_compiler_failures() {
    let root = std::env::var("QS400_OSS_DIR").map_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("oss/src"), std::path::PathBuf::from);
    let Ok(rd) = std::fs::read_dir(&root) else {
        eprintln!("oss: no projects in {}; nothing to compile", root.display());
        return;
    };
    let mut failures = Vec::new();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let (mut all, mut all_created) = (0, 0);
    let mut projects: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    projects.sort();
    for proj in projects {
        let mut files = Vec::new();
        walk(&proj, &mut files);
        files.sort();
        let members: Vec<&PathBuf> = files
            .iter()
            .filter(|p| MEMBER.contains(&ext(p).as_str()))
            .filter(|p| !p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("headers")))
            .collect();
        let mut created = 0;
        for p in &members {
            let text = String::from_utf8_lossy(&std::fs::read(p).expect("a member")).to_string();
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
            let name: String = stem.chars().filter(char::is_ascii_alphanumeric).take(10).collect();
            let case = Case::parse(&name, &text);
            let mut opts = rpg::options(&case);
            // A NOMAIN module is a module; anything else is a program.
            opts.command = if text.to_ascii_lowercase().contains("nomain") {
                rpgc::Command::CrtRpgMod
            } else {
                rpgc::Command::CrtBndRpg
            };
            let mut inc = ByStem::new(&files);
            let member = Member::from_text("OSS", "QRPGLESRC", &name, &text);
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rpgc::compile(member, &mut inc, &mut rpgc::NoExtFiles, &opts)
            }));
            let shown = p.strip_prefix(&root).unwrap_or(p).display().to_string();
            match r {
                Err(_) => failures.push(format!("{shown}: the compiler panicked")),
                Ok(c) => {
                    if let Some(d) = c.diags.iter().find(|d| d.msgid == "RNF0001") {
                        failures.push(format!("{shown}: {}", d.args.join(" ")));
                    }
                    if c.created {
                        created += 1;
                    } else if std::env::var("QS400_OSS_WHY").is_ok()
                        && let Some(d) = c.diags.iter().find(|d| d.severity > 10)
                    {
                        eprintln!("why: {shown}: {} {} at {}", d.msgid, d.args.join(" "), d.at.line);
                    }
                    for d in c.diags.iter().filter(|d| d.severity > 10) {
                        *ids.entry(d.msgid.to_string()).or_default() += 1;
                        if std::env::var("QS400_OSS_DETAIL").is_ok_and(|v| v == d.msgid) {
                            eprintln!("detail: {} {} {}:{}", d.msgid, d.args.join(" "), shown, d.at.line);
                        }
                    }
                }
            }
        }
        eprintln!("oss: {}: {created} of {} members created", proj.file_name().unwrap_or_default().to_string_lossy(), members.len());
        all += members.len();
        all_created += created;
    }
    let mut top: Vec<(String, usize)> = ids.into_iter().collect();
    top.sort_by_key(|x| std::cmp::Reverse(x.1));
    eprintln!("oss: {all_created} of {all} created; most common errors: {:?}", &top[..top.len().min(12)]);
    assert!(failures.is_empty(), "compiler failures:\n  {}", failures.join("\n  "));
}
