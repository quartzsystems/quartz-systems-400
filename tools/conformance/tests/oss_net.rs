// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTPAPI from the open-source corpus (`oss/src/httpapi`, fetched; see
//! `docs/PHASE9.md`): the service program built in memory, then its
//! examples called against the harness host's HTTP server. The build
//! report and each example's report are compared with
//! `oss/expected/httpapi/`; `QS400_UPDATE_OSS=1` rewrites them. The
//! examples need real sockets, so they run on Linux only.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use conformance::ossnet::{self, EXAMPLES, Httpapi, server::Server};

fn expected_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oss/expected/httpapi")
}

fn check(name: &str, text: &str, failures: &mut Vec<String>) {
    let ours = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/conformance-oss/ours/httpapi");
    fs::create_dir_all(&ours).unwrap();
    fs::write(ours.join(format!("{name}.out")), text).unwrap();
    let path = expected_dir().join(format!("{name}.out"));
    if std::env::var_os("QS400_UPDATE_OSS").is_some() {
        fs::create_dir_all(expected_dir()).unwrap();
        fs::write(&path, text).unwrap();
        return;
    }
    match fs::read_to_string(&path) {
        Ok(want) if want.replace("\r\n", "\n") == text => {}
        Ok(want) => failures.push(format!("{name}: differs\n--- expected\n{want}\n--- ours\n{text}")),
        Err(_) => failures.push(format!("{name}: no expected file; QS400_UPDATE_OSS=1 writes it")),
    }
}

#[test]
fn httpapi_builds_and_its_examples_run() {
    let Some(mut h) = Httpapi::load(&ossnet::oss_root()) else {
        eprintln!("oss: httpapi not fetched; nothing to run");
        return;
    };
    let mut failures = Vec::new();
    // A member of one's own to run, for looking into a failure.
    if let Ok(p) = std::env::var("QS400_OSS_EXTRA") {
        let text = fs::read_to_string(&p).unwrap().replace("

", "
");
        h.members.insert("EXTRA".into(), text.lines().map(str::to_string).collect());
    }
    if std::env::var_os("QS400_OSS_ONLY").is_none() || std::env::var_os("QS400_OSS_LISTINGS").is_some() {
        let build = ossnet::build_report(&h);
        eprintln!("{build}");
        check("BUILD", &build, &mut failures);
    }
    if cfg!(target_os = "linux") || std::env::var_os("QS400_OSS_ONLY").is_some() {
        let srv = Server::start(Arc::new(ossnet::answer)).expect("the HTTP server");
        let only = std::env::var("QS400_OSS_ONLY").ok();
        // QS400_OSS_EXTRA: a member of one's own, run as EXTRA.
        let extra = ossnet::Example { name: "EXTRA", parms: "", files: &["/tmp/w.txt"] };
        for ex in EXAMPLES.iter().chain(std::iter::once(&extra)).filter(|e| only.as_deref().is_none_or(|o| o.split(',').any(|n| n == e.name)) && h.members.contains_key(e.name)) {
            match ossnet::run_example(&h, ex, &srv) {
                Ok(text) => {
                    eprintln!("{text}");
                    // The port changes from run to run.
                    let text = text.replace(&format!("127.0.0.1:{}", srv.port), "127.0.0.1:PORT");
                    check(ex.name, &text, &mut failures);
                }
                Err(e) => failures.push(format!("{}: {e}", ex.name)),
            }
        }
    } else {
        eprintln!("oss: the examples need sockets; they run on Linux");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
