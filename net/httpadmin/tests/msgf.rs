// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QHTTPSVR/QZHBMSG` is made from the product's table: the message
//! file source (`runtime/msgf/msgs/qzhbmsg.toml`) says what
//! `src/msgs.rs` says. `QS400_UPDATE_MSGF=1` rewrites the source.

use std::fmt::Write as _;
use std::path::Path;

use httpadmin::msgs::{self, MESSAGES};

/// The highest `&n` in `s`.
fn vars_in(s: &str) -> usize {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(1))
        .filter(|&i| b[i] == b'&' && b[i + 1].is_ascii_digit())
        .map(|i| usize::from(b[i + 1] - b'0'))
        .max()
        .unwrap_or(0)
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The source the table makes.
fn source() -> String {
    let mut out = String::from(
        "# QZHBMSG: HTTP Server Administration's messages, installed in library\n\
         # QHTTPSVR. Made from net/httpadmin/src/msgs.rs by its test\n\
         # (QS400_UPDATE_MSGF=1 cargo test -p httpadmin --test msgf); do not edit.\n\
         # Every text is ours.\n\
         name = \"QZHBMSG\"\n\
         text = \"HTTP Server Administration messages\"\n",
    );
    for m in MESSAGES {
        let n = vars_in(m.text).max(vars_in(m.cause)).max(vars_in(m.recovery));
        let _ = write!(
            out,
            "\n[[msg]]\nid = \"{}\"\nsev = {}\ntext = {}\nhelp = {}\n",
            m.id,
            m.severity,
            quoted(m.text),
            quoted(&msgs::help_text(m.cause, m.recovery))
        );
        if n > 0 {
            let vars: Vec<&str> = vec!["\"*CHAR *VARY 2\""; n];
            let _ = writeln!(out, "vars = [{}]", vars.join(", "));
        }
    }
    out
}

#[test]
fn message_file_matches_the_table() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/msgf/msgs/qzhbmsg.toml");
    let want = source();
    if std::env::var_os("QS400_UPDATE_MSGF").is_some() {
        std::fs::write(&path, &want).unwrap();
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(
        have == want,
        "runtime/msgf/msgs/qzhbmsg.toml differs from src/msgs.rs; QS400_UPDATE_MSGF=1 cargo test -p httpadmin --test msgf rewrites it"
    );
    let file = msgf::source::parse(msgf::source::QZHBMSG).unwrap();
    assert_eq!(file.name, "QZHBMSG");
    assert_eq!(file.msgs.len(), MESSAGES.len());
    for m in MESSAGES {
        let d = file.find(m.id).unwrap();
        assert_eq!(d.text, m.text);
        assert_eq!(u16::from(m.severity), d.severity);
        assert_eq!(msgs::split_help(&d.help), (m.cause.to_string(), m.recovery.to_string()), "{}", m.id);
    }
    let (lib, _, _) = msgf::source::PRODUCT_FILES[0];
    assert_eq!(lib, "QHTTPSVR");
}
