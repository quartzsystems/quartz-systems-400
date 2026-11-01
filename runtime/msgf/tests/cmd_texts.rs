// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The analyzer's diagnostics read as what they mean: `CPD0043` names a
//! keyword, `CPD0033` a command refused to a limited user.

use msgf::source::{QCPFMSG, parse};

#[test]
fn keyword_and_limited_texts() {
    let f = parse(QCPFMSG).unwrap();
    let kw = f.find("CPD0043").unwrap();
    assert_eq!(
        kw.format(&kw.data_of(&["INTETADR"])),
        "Keyword INTETADR is not valid for this command."
    );
    let lim = f.find("CPD0033").unwrap();
    assert_eq!(
        lim.format(&lim.data_of(&["CRTLIB"])),
        "Command CRTLIB is not allowed for a user with limited capabilities."
    );
}
