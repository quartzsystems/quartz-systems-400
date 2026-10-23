// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QRPGLEMSG` says what the compiler's listing says: every text in
//! `rpgc::msgs` is in the message file word for word, and the RPG
//! files look up as the runtime and the commands send them.

use msgf::source::{QRNXMSG, QRPGLEMSG, parse};

#[test]
fn listing_texts_match_the_message_file() {
    let f = parse(QRPGLEMSG).expect("QRPGLEMSG parses");
    let mut bad = Vec::new();
    for (id, text) in rpgc::msgs::texts() {
        match f.find(id) {
            None => bad.push(format!("{id}: not in qrpglemsg.toml")),
            Some(d) if d.text != *text => {
                bad.push(format!("{id}: listing {text:?}, file {:?}", d.text));
            }
            Some(_) => {}
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn rpg_files_hold_what_is_sent() {
    let c = parse(QRPGLEMSG).unwrap();
    for id in [
        "RNS9304", "RNS9305", "RNS9309", "RNS9310", "RNS9311", "RNS9314",
    ] {
        assert!(c.find(id).is_some(), "{id}");
    }
    assert!(
        c.msgs
            .iter()
            .all(|m| m.id.starts_with("RNF") || m.id.starts_with("RNS"))
    );
    let r = parse(QRNXMSG).unwrap();
    for id in ["RNX0100", "RNX0103", "RNX0105", "RNX9001", "RNX1218"] {
        assert!(r.find(id).is_some(), "{id}");
    }
    let q = r.find("RNQ0202").unwrap();
    assert_eq!(q.dft_reply.as_deref(), Some("C"));
    assert!(q.reply_ok("D") && !q.reply_ok("R"));
}
