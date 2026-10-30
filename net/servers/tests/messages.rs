// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message identifier the framework sends has a description in
//! `QTCPMSG`.

use std::path::Path;

#[test]
fn every_identifier_is_described() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/msgf/msgs/qtcpmsg.toml");
    let toml = std::fs::read_to_string(&p).unwrap();
    for id in servers::msg::ALL {
        assert!(toml.contains(&format!("id = \"{id}\"")), "{id} has no description in QTCPMSG");
    }
}
