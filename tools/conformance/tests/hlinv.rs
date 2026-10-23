// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HLINV: every RPG member of the application, unmodified, compiles
//! with nothing above severity 10, and where a listing captured on IBM i
//! sits beside it (`NAME.lst`), our diagnostics are the same. The source
//! is not ours to ship: it goes into `tools/conformance/hlinv/src/`
//! (gitignored), and the test says so and passes while it is absent.

use std::path::Path;

use conformance::rpg;
use rpg_oracle::case::Case;
use rpg_oracle::listing;

#[test]
#[ignore = "HLINV needs externally described files and display I/O; run with --ignored"]
fn hlinv_compiles_unmodified() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("hlinv/src");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("hlinv: no source in {}; nothing to compile", dir.display());
        return;
    };
    // The members, in the folder or one level down (`QRPGLESRC/`).
    let mut found = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()));
        } else {
            found.push(p);
        }
    }
    let mut members: Vec<_> = found
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("rpgle") || x.eq_ignore_ascii_case("sqlrpgle"))
        })
        .collect();
    members.sort();
    let mut bad = Vec::new();
    for p in &members {
        let text = String::from_utf8_lossy(&std::fs::read(p).expect("a member")).to_string();
        let name = p.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
        let mut case = Case::parse(&name, &text);
        case.call.clear();
        let c = rpg::compile(&case);
        let ours: Vec<String> = rpg::diags(&c).iter().map(|d| format!("{} {:02} {}", d.msgid, d.severity, d.line)).collect();
        let worst = c.diags.iter().map(|d| d.severity).max().unwrap_or(0);
        if !c.created || worst > 10 {
            bad.push(format!("{name}: not created or severity {worst}: {}", ours.join(", ")));
            continue;
        }
        let lst = p.with_extension("lst");
        if let Ok(bytes) = std::fs::read(&lst) {
            let theirs: Vec<String> = listing::diags(&String::from_utf8_lossy(&bytes).lines().map(str::to_string).collect::<Vec<_>>())
                .iter()
                .map(|d| format!("{} {:02} {}", d.msgid, d.severity, d.line))
                .collect();
            if theirs != ours {
                bad.push(format!("{name}: diagnostics differ: ours {ours:?}, IBM's {theirs:?}"));
            }
        }
    }
    eprintln!("hlinv: {} members, {} clean", members.len(), members.len() - bad.len());
    assert!(bad.is_empty(), "hlinv:\n  {}", bad.join("\n  "));
}
