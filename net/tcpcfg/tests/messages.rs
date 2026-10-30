// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message identifier the crate sends has a description in
//! `QTCPMSG`, and those taken from the oracle's `QTCPMSGF` keep its
//! substitution layout.

use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn every_identifier_is_described() {
    let toml = read("../../runtime/msgf/msgs/qtcpmsg.toml");
    for id in tcpcfg::msg::ALL {
        assert!(toml.contains(&format!("id = \"{id}\"")), "{id} has no description in QTCPMSG");
    }
}

#[test]
fn oracle_identifiers_keep_their_layouts() {
    let toml = read("../../runtime/msgf/msgs/qtcpmsg.toml");
    // QSYS/QTCPMSG itself where it was captured (the PING and
    // TRACEROUTE identifiers), else QTCP/QTCPMSGF.
    let qsys = read("../../tools/conformance/oracle/net/pub400/messages-qtcpmsg-tools.txt");
    let oracle = read("../../tools/conformance/oracle/net/pub400/messages-tcp.txt");
    for block in toml.split("[[msg]]").skip(1) {
        let id = block.split('"').nth(1).unwrap();
        let mut lines = qsys.lines().chain(oracle.lines());
        let Some(line) = lines.find(|l| l.starts_with(&format!("{id}|"))) else {
            continue;
        };
        let f: Vec<&str> = line.split('|').collect();
        let theirs: Vec<String> = if f[3] == "-" {
            Vec::new()
        } else {
            f[3].split('&')
                .filter(|s| !s.is_empty())
                .map(|s| s.split_once(' ').unwrap().1.trim().to_string())
                // The message file has no unsigned binary or varying
                // quoted fields: the signed and plain forms stand in.
                .map(|t| t.replace("*UBIN", "*BIN").replace("*QTDCHAR *VARY", "*CHAR *VARY"))
                .collect()
        };
        let ours: Vec<String> = block
            .lines()
            .find(|l| l.starts_with("vars"))
            .map(|l| {
                l.split('"')
                    .enumerate()
                    .filter(|(i, _)| i % 2 == 1)
                    .map(|(_, s)| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(ours, theirs, "{id}");
    }
}
