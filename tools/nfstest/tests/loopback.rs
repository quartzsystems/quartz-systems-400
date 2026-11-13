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
