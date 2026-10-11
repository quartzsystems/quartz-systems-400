// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database kit in memory: QSTEST from its DDS, the data loaded, the
//! script and queries run, the outputs written and compared with the
//! machine's where they exist.

use std::fs;

use conformance::db::{Kit, Local, compare, rpg};

#[test]
fn kit_builds_runs_and_compares() {
    let kit = Kit::locate();
    let mut local = Local::build(&kit).expect("build QSTEST");
    let loaded = local.load(&kit).expect("load");
    assert!(loaded.iter().all(|(_, r)| r.errors == 0));
    local.start_journal().expect("journal");
    let report = local.run_rla(&kit).expect("run the script");
    assert_eq!(report.bad, 0, "script mismatches:\n{}", report.text);
    let queries = local.run_queries(&kit).expect("queries");
    for (i, (q, r)) in queries.iter().enumerate() {
        assert!(
            r.is_ok(),
            "query {}: {}\n  {q}",
            i + 1,
            r.as_ref().unwrap_err()
        );
    }
    let ours = Kit::ours_dir();
    let _ = fs::remove_dir_all(&ours);
    let written = local.dump(&kit, &report, &queries, &ours).expect("dump");
    assert!(written.len() > 20, "{} files written", written.len());

    // The oracle programs follow the script and queries.
    let update = std::env::var_os("QS400_UPDATE_ORACLE").is_some();
    for (name, text) in [
        ("RLATEST.RPGLE", rpg::generate_rpg(&kit).unwrap()),
        ("QRESULTS.SQL", rpg::generate_sql(&kit).unwrap()),
    ] {
        let p = kit.oracle_dir().join(name);
        if update || !p.is_file() {
            fs::write(&p, &text).unwrap();
        } else {
            let have = fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            assert_eq!(
                have, text,
                "{name} is stale; QS400_UPDATE_ORACLE=1 cargo test -p conformance"
            );
        }
    }

    let c = compare(&ours, &kit.expected_dir());
    eprintln!("{}", c.render());
    assert!(
        c.mismatched.is_empty(),
        "outputs differ from the machine's:\n{}",
        c.render()
    );
}
