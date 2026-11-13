// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS version 4 scenarios that need no file data, through the command
//! line's entry point against the scripted responder (`v4fake`); and every
//! version 4 scenario against QS/400's server, once it serves version 4.

mod common;
mod v4fake;

fn run(args: &[String]) -> (i32, String) {
    let mut out = Vec::new();
    let code = nfstest::cli::run(args, &mut out);
    (code, String::from_utf8(out).unwrap())
}

fn args(port: u16, vers: &str, only: &[&str]) -> Vec<String> {
    let mut a: Vec<String> = [
        "127.0.0.1",
        "/home/exp",
        "scratch",
        "--vers",
        vers,
        "--port",
        &port.to_string(),
        "--pmap-port",
        "9",
        "--timeout",
        "3",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    if !only.is_empty() {
        a.push("--only".into());
        a.push(only.join(","));
    }
    a
}

const V40: &[&str] = &[
    "v4-null",
    "v4-minor2-refused",
    "v4-op-illegal",
    "v40-setclientid-renew",
    "v40-lookup-export",
    "v40-sequence-refused",
];

const V41: &[&str] = &[
    "v4-null",
    "v4-minor2-refused",
    "v4-op-illegal",
    "v41-exchange-session",
    "v41-lookup-export",
    "v41-slot-replay",
    "v41-misordered-sequence",
    "v41-no-sequence-refused",
    "v41-acl-not-returned",
];

fn all_ok(out: &str, names: &[&str]) {
    for n in names {
        assert!(
            out.lines().any(|l| l == format!("ok {n}")),
            "{n} did not pass:\n{out}"
        );
    }
}

#[test]
fn version_4_0_against_the_scripted_responder() {
    let f = v4fake::start();
    let (code, out) = run(&args(f.port, "4.0", V40));
    println!("{out}");
    all_ok(&out, V40);
    assert_eq!(code, 0);
}

#[test]
fn version_4_1_against_the_scripted_responder() {
    let f = v4fake::start();
    let (code, out) = run(&args(f.port, "4.1", V41));
    println!("{out}");
    all_ok(&out, V41);
    assert_eq!(code, 0);
}

#[test]
fn a_version_3_server_refuses_version_4() {
    let s = common::start(&common::Options::default());
    let pmap = s.pmap_port.to_string();
    let mut a = args(0, "4.0", &["v4-null"]);
    // The binder's NFS port, not --port.
    let at = a.iter().position(|x| x == "--port").unwrap();
    a.drain(at..at + 2);
    let p = a.iter().position(|x| x == "--pmap-port").unwrap();
    a[p + 1] = pmap;
    let (_, out) = run(&a);
    println!("{out}");
    assert!(
        out.contains("fail v4-null: rpc: PROG_MISMATCH 3..3"),
        "{out}"
    );
}

fn real(vers: &str) {
    let s = common::start(&common::Options {
        v4: true,
        ..common::Options::default()
    });
    let pmap = s.pmap_port.to_string();
    let mut a = args(0, vers, &[]);
    let at = a.iter().position(|x| x == "--port").unwrap();
    a.drain(at..at + 2);
    let p = a.iter().position(|x| x == "--pmap-port").unwrap();
    a[p + 1] = pmap;
    let (code, out) = run(&a);
    println!("{out}");
    assert_eq!(code, 0, "{out}");
}

#[test]
fn every_4_0_scenario_against_the_server() {
    real("4.0");
}

#[test]
fn every_4_1_scenario_against_the_server() {
    real("4.1");
}

#[test]
fn bench_over_4_0_and_4_1() {
    let s = common::start(&common::Options {
        v4: true,
        ..common::Options::default()
    });
    let pmap = s.pmap_port.to_string();
    for vers in ["4.0", "4.1"] {
        let mut a = vec!["bench".to_string()];
        a.extend(args(0, vers, &[]));
        let at = a.iter().position(|x| x == "--port").unwrap();
        a.drain(at..at + 2);
        let p = a.iter().position(|x| x == "--pmap-port").unwrap();
        a[p + 1].clone_from(&pmap);
        a.extend(["--mib".into(), "4".into()]);
        let (code, out) = run(&a);
        println!("{vers}: {out}");
        assert_eq!(code, 0, "{vers}: {out}");
        assert!(
            out.contains("read MiB/s: ") && out.contains("block: 65536"),
            "{out}"
        );
    }
}
