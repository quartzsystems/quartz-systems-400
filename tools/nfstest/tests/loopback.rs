// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every NFS version 3, MOUNT, NLM and NSM scenario through the command
//! line's entry point against QS/400's server on loopback (`common`).

mod common;

/// Scenarios that fail against the server on main: findings for the
/// server's owner, each in `tools/nfstest/README.md` ("Findings").
const KNOWN: &[&str] = &[
    // A daemon thread's cached write descriptor holds a share reservation
    // until the thread's next call (ifsvfs::fdcache's lazy sweep).
    "nlm-share",
];

fn run(args: &[&str]) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut out = Vec::new();
    let code = nfstest::cli::run(&args, &mut out);
    (code, String::from_utf8(out).unwrap())
}

#[test]
fn every_v3_scenario_against_the_server() {
    let cb = common::free_port();
    let s = common::start(&common::Options {
        callback_nlm: Some(cb),
        ..common::Options::default()
    });
    let pmap = s.pmap_port.to_string();
    let cbs = cb.to_string();
    let only = std::env::var("NFSTEST_ONLY").unwrap_or_default();
    let mut args = vec![
        "127.0.0.1",
        common::EXPORT,
        "scratch",
        "--pmap-port",
        &pmap,
        "--uid",
        "100",
        "--gid",
        "100",
        "--callbacks",
        "--cb-pmap-port",
        "0",
        "--cb-nlm-port",
        &cbs,
        "--large-mib",
        "4",
    ];
    if !only.is_empty() {
        args.extend(["--only", &only]);
    }
    let (code, out) = run(&args);
    println!("{out}");
    let failed: Vec<&str> = out
        .lines()
        .filter_map(|l| l.strip_prefix("fail "))
        .filter_map(|l| l.split(':').next())
        .collect();
    let new: Vec<&&str> = failed.iter().filter(|f| !KNOWN.contains(f)).collect();
    assert!(
        new.is_empty(),
        "scenarios failing against the server: {new:?}"
    );
    if only.is_empty() {
        let fixed: Vec<&&str> = KNOWN.iter().filter(|k| !failed.contains(k)).collect();
        assert!(
            fixed.is_empty(),
            "now passing, take them out of KNOWN: {fixed:?}"
        );
        assert_eq!(code, i32::try_from(KNOWN.len()).unwrap());
    }
}

#[test]
fn probe_lists_the_daemons_and_the_export() {
    let s = common::start(&common::Options::default());
    let pmap = s.pmap_port.to_string();
    let (code, out) = run(&["probe", "127.0.0.1", "--pmap-port", &pmap]);
    println!("{out}");
    assert_eq!(code, 0);
    for want in ["portmapper", "nfs", "mountd", "nlockmgr", "status"] {
        assert!(out.contains(want), "{want} missing:\n{out}");
    }
    assert!(
        out.contains(&format!("Export list for 127.0.0.1:\n{}", common::EXPORT)),
        "{out}"
    );
}

#[test]
fn bench_writes_and_reads_back() {
    let s = common::start(&common::Options::default());
    let pmap = s.pmap_port.to_string();
    let (code, out) = run(&[
        "bench",
        "127.0.0.1",
        common::EXPORT,
        "scratch",
        "--pmap-port",
        &pmap,
        "--uid",
        "100",
        "--gid",
        "100",
        "--mib",
        "8",
    ]);
    println!("{out}");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("write MiB/s: ") && out.contains("read MiB/s: "),
        "{out}"
    );
}

#[test]
fn the_grace_period_after_a_start() {
    let s = common::start(&common::Options {
        grace: std::time::Duration::from_secs(60),
        ..common::Options::default()
    });
    let pmap = s.pmap_port.to_string();
    let (code, out) = run(&[
        "127.0.0.1",
        common::EXPORT,
        "scratch",
        "--pmap-port",
        &pmap,
        "--uid",
        "100",
        "--gid",
        "100",
        "--expect-grace",
        "--only",
        "nlm-grace",
    ]);
    println!("{out}");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok nlm-grace"), "{out}");
}
