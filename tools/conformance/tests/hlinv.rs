// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HLINV: every RPG member of the application, unmodified, compiles
//! with nothing above severity 10, and where a listing captured on IBM i
//! sits beside it (`NAME.lst`), our diagnostics are the same. The
//! compiler reads the files' formats from the application's own DDS
//! (`QDDSSRC/`: physical and logical files, display and printer files),
//! built in memory as the corpus builds its files. The source is not
//! ours to ship: it goes into `tools/conformance/hlinv/src/`
//! (gitignored), and the test says so and passes while it is absent.

use std::path::Path;

use conformance::rpg;
use rpg_oracle::case::{Case, DbStep, DdsKind};
use rpg_oracle::listing;

#[test]
#[ignore = "HLINV does not compile clean yet (PHASE9.md, Blocked); run with --ignored"]
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
    // The application's files, physical before the files over them.
    let mut steps = Vec::new();
    for kind in [DdsKind::Pf, DdsKind::Lf, DdsKind::Dspf, DdsKind::Prtf] {
        let mut names: Vec<_> = std::fs::read_dir(dir.join("QDDSSRC"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case(kind.ext())))
            .collect();
        names.sort();
        for p in names {
            let text = String::from_utf8_lossy(&std::fs::read(&p).expect("DDS")).replace("\r\n", "\n");
            steps.push(DbStep::Dds {
                name: p.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default(),
                kind,
                source: text.lines().map(str::to_string).collect(),
            });
        }
    }
    // A file whose DDS does not compile is left out and reported.
    let mut kept: Vec<DbStep> = Vec::new();
    for st in steps {
        let mut trial = kept.clone();
        trial.push(st.clone());
        match rpg::files::KitDb::build(&trial) {
            Ok(_) => kept = trial,
            Err(e) => {
                let first = e.lines().find(|l| l.contains("DDS") && l.contains(" 20 ") || l.contains(" 30 ")).unwrap_or("");
                if let DbStep::Dds { name, .. } = &st {
                    eprintln!("hlinv: {name} left out: {}", first.trim());
                }
            }
        }
    }
    let db = rpg::files::KitDb::build(&kept).ok();
    let mut bad = Vec::new();
    for p in &members {
        let text = String::from_utf8_lossy(&std::fs::read(p).expect("a member")).to_string();
        let name = p.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
        let mut case = Case::parse(&name, &text);
        case.call.clear();
        let c = rpg::compile(&case, db.as_ref());
        let ours: Vec<String> = rpg::diags(&c).iter().map(|d| format!("{} {:02} {}", d.msgid, d.severity, d.line)).collect();
        let worst = c.diags.iter().map(|d| d.severity).max().unwrap_or(0);
        if !c.created || worst > 10 {
            // The compiler's own failures say what they are.
            let why: Vec<String> = c.diags.iter().filter(|d| d.severity >= 50).map(rpgc::msgs::text).collect();
            bad.push(format!("{name}: not created or severity {worst}: {} {}", ours.join(", "), why.join("; ")));
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
