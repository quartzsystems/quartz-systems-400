// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media messages against their `QCPFMSG` entries: the identifier,
//! data exactly as long as the entry's fields, and the text with every
//! `&n` filled.

use medio::MediaError;
use medio::msg::{self, Msg};
use msgf::{BuiltinMsgs, MsgLookup};
use tape::{TapeError, TransportError};

/// The entry's text with `m`'s data, after checking the data's length
/// against the declared fields.
fn render(m: &Msg) -> (String, String) {
    let d = BuiltinMsgs::new()
        .describe(m.id, "QCPFMSG", "QSYS")
        .unwrap_or_else(|| panic!("{} is not in QCPFMSG", m.id));
    let declared: usize = d.vars.iter().map(msgf::desc::VarType::fixed_size).sum();
    assert_eq!(m.data.len(), declared, "{}: data and fields", m.id);
    let (text, help) = (d.format(&m.data), d.format_help(&m.data));
    assert!(!text.contains('&') && !help.contains('&'), "{text} / {help}");
    (text, help)
}

fn active() -> MediaError {
    MediaError::Tape(TapeError::ActiveFile {
        volume: "ANS001".into(),
        seq: 2,
        label: "SMALL".into(),
        expires: "*PERM".into(),
    })
}

#[test]
fn inztap_labeled_and_nonlabeled() {
    let m = msg::tape_initialized("TAP01", Some("PRT001"), "*DEVTYPE");
    assert_eq!(m.id, "CPC6773");
    assert_eq!(m.data, msg::data(&[msg::Field::Char("TAP01", 10), msg::Field::Char("PRT001", 6)]));
    let (text, _) = render(&m);
    assert!(text.contains("PRT001") && text.contains("TAP01"), "{text}");

    let m = msg::tape_initialized("TAP01", None, "*DEVTYPE");
    assert_eq!(m.id, "CPC6776");
    assert_eq!(m.data.len(), 20);
    assert_eq!(&m.data[10..13], &[0x5c, 0xc4, 0xc5], "&4 the density");
    let (text, help) = render(&m);
    assert!(text.contains("TAP01") && help.contains("*DEVTYPE"), "{help}");
}

#[test]
fn write_protected() {
    let e = MediaError::Tape(TapeError::Transport(TransportError::WriteProtected));
    let m = msg::escape(&e, "TAP01", "PRT001");
    assert_eq!(m.id, "CPF6768");
    assert_eq!(m.data, msg::data(&[msg::Field::Char("TAP01", 10)]));
    assert!(m.detail.is_empty(), "the message says it all");
    assert!(render(&m).0.contains("TAP01"));
}

#[test]
fn active_file_escape_and_inquiry() {
    let e = active();
    let m = msg::escape(&e, "TAP01", "ANS001");
    assert_eq!(m.id, "CPF6754");
    let want = msg::data(&[
        msg::Field::Char("TAP01", 10),
        msg::Field::Char("ANS001", 6),
        msg::Field::Char("", 6),
        msg::Field::Char("SMALL", 17),
        msg::Field::Char("*PERM", 10),
        msg::Field::Char("2", 10),
    ]);
    assert_eq!(m.data, want);
    let (text, help) = render(&m);
    assert!(text.contains("SMALL") && text.contains("ANS001"), "{text}");
    assert!(help.contains("*PERM") && help.contains("TAP01"), "{help}");

    let q = msg::active_file_inquiry(&e, "TAP01").expect("an inquiry");
    assert_eq!(q.id, "CPA6754");
    assert_eq!(q.data, want);
    render(&q);
    let other = MediaError::Tape(TapeError::EndOfVolume);
    assert!(msg::active_file_inquiry(&other, "TAP01").is_none());
}

#[test]
fn next_volume_inquiries() {
    let m = msg::next_volume("TAPVRT01", None);
    assert_eq!(m.id, "CPA4088");
    assert_eq!(&m.data[30..], msg::data(&[msg::Field::Char("TAPVRT01", 10)]).as_slice());
    assert!(render(&m).0.contains("TAPVRT01"));
    let m = msg::next_volume("TAPVRT01", Some("VOL002"));
    assert_eq!(m.id, "CPA4089");
    assert_eq!(&m.data[40..], msg::data(&[msg::Field::Char("VOL002", 6)]).as_slice());
    let (text, _) = render(&m);
    assert!(text.contains("VOL002") && text.contains("TAPVRT01"), "{text}");
}

#[test]
fn every_error_fits_its_entry() {
    use tape::device::LibraryError as L;
    let errors = [
        MediaError::Tape(TapeError::Transport(TransportError::NotLoaded)),
        MediaError::Tape(TapeError::Transport(TransportError::EndOfMedium)),
        MediaError::Tape(TapeError::WrongVolume {
            expected: "A".into(),
            found: "B".into(),
        }),
        MediaError::Tape(TapeError::NotLabeled),
        MediaError::Tape(TapeError::FileNotFound { what: "3".into() }),
        MediaError::Tape(TapeError::Cancelled),
        MediaError::Tape(TapeError::EndOfVolume),
        MediaError::Library(L::NotFound("C1".into())),
        MediaError::Library(L::Category {
            id: "C1".into(),
            category: "*INSERT".into(),
        }),
        MediaError::Library(L::BadCategory("*IPL".into())),
        MediaError::Library(L::SysGen("TAPMLB01".into())),
        MediaError::Library(L::InUse("C1".into())),
        MediaError::Library(L::NoDrive("TAPMLB01".into())),
        MediaError::Library(L::NoSpace("slot")),
        MediaError::Optical(optical::Error::Malformed("x".into())),
        MediaError::Optical(optical::Error::NotFound("/A".into())),
        MediaError::Optical(optical::Error::Exists("/A".into())),
        MediaError::Optical(optical::Error::NotDir("/A".into())),
        MediaError::Optical(optical::Error::IsDir("/A".into())),
        MediaError::Optical(optical::Error::NotEmpty("/A".into())),
        MediaError::Optical(optical::Error::BadName("A*".into())),
        MediaError::Optical(optical::Error::BadVolumeId("1*".into())),
        MediaError::Optical(optical::Error::VolumeFull {
            need: 2,
            capacity: 1,
        }),
        MediaError::Optical(optical::Error::WriteProtected),
        MediaError::Optical(optical::Error::ReadOnly),
        MediaError::Optical(optical::Error::ActiveFiles),
        MediaError::Optical(optical::Error::Invalid("x".into())),
        MediaError::NotFound("/LIB".into()),
        MediaError::Exists("/LIB".into()),
        MediaError::State("x".into()),
    ];
    for e in &errors {
        let m = msg::escape(e, "TAP01", "VOL001");
        assert_eq!(m.id, e.msgid());
        render(&m);
    }
}
