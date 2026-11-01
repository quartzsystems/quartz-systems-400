// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The analyzer's diagnostics read as what they mean: `CPD0043` names a
//! keyword, `CPD0033` a command refused to a limited user, `CPD0032` a
//! command the user is not authorized to, `CPD0034` a mode mismatch;
//! `CPF0006` ends them.

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
    let aut = f.find("CPD0032").unwrap();
    assert_eq!(
        aut.format(&aut.data_of(&["RSTLIB", "*LIBL"])),
        "You may not use command RSTLIB in library *LIBL."
    );
    let mode = f.find("CPD0034").unwrap();
    assert!(mode.format(&mode.data_of(&["STRSST"])).contains("mode"));
    let end = f.find("CPF0006").unwrap();
    assert_eq!(end.format(&[]), "The command had errors.");
}
