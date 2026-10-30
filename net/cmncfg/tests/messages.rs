// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message identifier the crate names is in `QCPFMSG`, and the
//! data the crate builds fills the variables the description lists.

use cmncfg::msgs;
use msgf::MsgLookup as _;

const IDS: &[&str] = &[
    msgs::LINE_CREATED,
    msgs::LINE_CHANGED,
    msgs::LINE_DELETED,
    msgs::DEV_DELETED,
    msgs::CTL_DELETED,
    msgs::DEV_VARIED_ON,
    msgs::DEV_VARIED_OFF,
    msgs::LINE_VARIED_ON,
    msgs::LINE_VARIED_OFF,
    msgs::CTL_VARIED_ON,
    msgs::CTL_VARIED_OFF,
    msgs::AUTO_CTL,
    msgs::AUTO_DEV,
    msgs::AUTO_CTL_DELETED,
    msgs::AUTO_DEV_DELETED,
    msgs::AUTO_CTL_FAILED,
    msgs::AUTO_DEV_FAILED,
    msgs::LINE_NOT_FOUND,
    msgs::CTL_NOT_FOUND,
    msgs::DEV_NOT_FOUND,
    msgs::LINE_EXISTS,
    msgs::LINE_NOT_CHANGED,
    msgs::MUST_VARY_OFF,
    msgs::DELETE_VARIED_ON,
    msgs::VARY_NOT_PROCESSED,
    msgs::LINE_ALREADY_ON,
    msgs::LINE_ALREADY_OFF,
    msgs::CTL_ALREADY_OFF,
    msgs::DEV_ALREADY_OFF,
    msgs::RSRC_NOT_FOUND,
    msgs::LINE_NOT_VARIED_ON,
    msgs::CTL_NOT_ON,
    msgs::LINE_NOT_ON,
    msgs::CONTACTED,
    msgs::RECOVERY_STARTED,
    msgs::LINE_FAILED,
];

#[test]
fn every_identifier_is_described() {
    let mut m = msgf::BuiltinMsgs::new();
    for id in IDS {
        assert!(
            m.describe(id, "QCPFMSG", "QSYS").is_some(),
            "{id} is not in QCPFMSG"
        );
    }
}

#[test]
fn the_data_fills_the_text() {
    let mut m = msgf::BuiltinMsgs::new();
    let mut text = |id: &str| {
        let d = m.describe(id, "QCPFMSG", "QSYS").unwrap();
        d.format(&msgs::data(id, "ETHLINE", "ETHLINET"))
    };
    let failed = text(msgs::LINE_FAILED);
    assert!(
        failed.contains("ETHLINE") && !failed.contains('&'),
        "{failed}"
    );
    let contacted = text(msgs::CONTACTED);
    assert!(
        contacted.contains("ETHLINE") && contacted.contains("ETHLINET"),
        "{contacted}"
    );
    assert!(text(msgs::RECOVERY_STARTED).contains("ETHLINET"));
    assert!(text(msgs::LINE_CREATED).contains("ETHLINE"));
    let d = m.describe(msgs::LINE_FAILED, "QCPFMSG", "QSYS").unwrap();
    assert_eq!(d.dft_reply.as_deref(), Some("C"));
    assert_eq!(d.vars.len(), 40);
}
