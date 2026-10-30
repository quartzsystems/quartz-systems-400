// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The FTP transcript replay: every recorded session against our
//! server in memory. Writes the per-category table and every
//! difference to `target/conformance-ftp/`; fails when fewer sessions
//! pass than `ftp/baseline.txt` says.

use std::fmt::Write as _;
use std::path::Path;

use conformance::ftp;

#[test]
fn replay_transcripts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("ftp");
    let fixture = std::fs::read_to_string(root.join("fixture.txt")).expect("fixture");
    let gaps = std::fs::read_to_string(root.join("gaps.txt")).unwrap_or_default();
    let transcripts = ftp::load(&root.join("transcripts"));
    assert!(!transcripts.is_empty(), "no transcripts");
    let outcomes: Vec<ftp::Outcome> = transcripts.iter().map(|t| ftp::replay(t, &fixture, &gaps)).collect();
    let table = ftp::report(&outcomes);
    let mut details = String::new();
    for o in &outcomes {
        if !o.passed() {
            let _ = writeln!(details, "## {}", o.name);
            for d in &o.diffs {
                let _ = writeln!(details, "- {d}");
            }
        }
    }
    let out = conformance::target_dir().join("conformance-ftp");
    let _ = std::fs::create_dir_all(&out);
    let _ = std::fs::write(out.join("report.md"), format!("{table}\n{details}"));
    println!("{table}");
    let passed = outcomes.iter().filter(|o| o.passed()).count();
    let baseline: usize = std::fs::read_to_string(root.join("baseline.txt"))
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.trim().parse().ok()))
        .unwrap_or(0);
    assert!(
        passed >= baseline,
        "{passed} sessions pass, the baseline is {baseline}; see {}",
        out.join("report.md").display()
    );
}
