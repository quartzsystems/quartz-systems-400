// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message the tools send has a description: the `TCP` ones in
//! `QTCPMSG`, the communications trace's `CPF39xx` in `QCPFMSG`, with
//! the oracle's substitution layouts (`messages-qtcpmsg-tools.txt`,
//! `messages-cmntrc.txt`).

use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn block<'a>(toml: &'a str, id: &str) -> Option<&'a str> {
    toml.split("[[msg]]").find(|b| b.contains(&format!("id = \"{id}\"")))
}

fn vars(block: &str) -> Vec<String> {
    block
        .lines()
        .find(|l| l.starts_with("vars"))
        .map(|l| {
            l.split('"')
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
                .map(|(_, s)| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn layout(capture: &str, id: &str) -> Option<Vec<String>> {
    let line = capture.lines().find(|l| l.starts_with(&format!("{id}|")))?;
    let f: Vec<&str> = line.split('|').collect();
    Some(if f[3] == "-" {
        Vec::new()
    } else {
        f[3].split('&')
            .filter(|s| !s.is_empty())
            .map(|s| s.split_once(' ').unwrap().1.trim().to_string())
            .map(|t| t.replace("*UBIN", "*BIN").replace("*QTDCHAR *VARY", "*CHAR *VARY"))
            .collect()
    })
}

#[test]
fn every_identifier_is_described_with_the_oracle_layout() {
    let tcp = read("../../runtime/msgf/msgs/qtcpmsg.toml");
    let cpf = read("../../runtime/msgf/msgs/qcpfmsg.toml");
    let tcp_oracle = read("../../tools/conformance/oracle/net/pub400/messages-qtcpmsg-tools.txt");
    let cpf_oracle = read("../../tools/conformance/oracle/net/pub400/messages-cmntrc.txt");
    for id in nettools::msg::ALL {
        let (toml, oracle) = if id.starts_with("TCP") { (&tcp, &tcp_oracle) } else { (&cpf, &cpf_oracle) };
        let b = block(toml, id).unwrap_or_else(|| panic!("{id} has no description"));
        let theirs = layout(oracle, id).unwrap_or_else(|| panic!("{id} is not in the oracle's capture"));
        assert_eq!(vars(b), theirs, "{id}");
    }
}
