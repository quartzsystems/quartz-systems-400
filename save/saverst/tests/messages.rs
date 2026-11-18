// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The engine's messages carry exactly the data their catalogue entries
//! declare, and their text renders from it.

use msgf::MsgDesc;
use msgf::source::{QCPFMSG, parse};
use saverst::msg::{self, Dest, Note};
use saverst::{ObjResult, RestoreReport, SaveReport};
use store::obj::types;

/// The data fills every declared variable, and nothing more.
fn fits(d: &MsgDesc, data: &[u8]) {
    let mut at = 0;
    for v in &d.vars {
        let (_, n) = v.render(&data[at.min(data.len())..]);
        assert!(n > 0 || v.fixed_size() == 0, "{}: short at {at}", d.id);
        at += n;
    }
    assert_eq!(at, data.len(), "{}: {} bytes declared", d.id, at);
}

fn object(name: &str, lib: &str, msgid: Option<&'static str>) -> ObjResult {
    ObjResult {
        library: lib.into(),
        name: name.into(),
        type_code: types::FILE,
        msgid,
        ..ObjResult::default()
    }
}

#[test]
fn object_messages_fit_their_entries() {
    let f = parse(QCPFMSG).unwrap();
    let ids = [
        msg::NO_LIBRARY,
        msg::NOT_FOUND_SAVE,
        msg::IN_USE,
        msg::NOT_AUTHORIZED,
        msg::SAVF_NO_DATA,
        msg::NOT_RESTORED,
        msg::SKIP_EXISTS,
        msg::SKIP_MISSING,
        msg::SKIP_NOT_FREED,
        msg::JOURNAL_EXISTS,
        msg::DTAQ_EXISTS,
        msg::RECEIVER_ATTACHED,
        msg::RECEIVER_EXISTS,
        msg::NOT_RESTORED_BASED_ON,
        msg::NOT_RESTORED_MEMBERS,
        msg::NOT_RESTORED_LEVEL,
        msg::NOT_RESTORED_OWNER,
        msg::NOT_RESTORED_PGP,
        msg::NOT_RESTORED_AUTL,
        msg::NOT_RESTORED_DAMAGED,
        msg::NONE_IN_SAVE,
        msg::DEFAULT_OWNER,
        msg::JOURNAL_MISSING,
        msg::NOT_RESTORED_SYSTEM_STATE,
        msg::NOT_RESTORED_ADOPTS,
        msg::NOT_RESTORED_SIGNATURE,
    ];
    for id in ids {
        let d = f.find(id).unwrap_or_else(|| panic!("{id}"));
        let (_, data) = msg::of_result(&object("CUST", "QGPL", Some(id))).unwrap();
        fits(d, &data);
        let shown = d.format(&data);
        assert!(!shown.contains('&'), "{id}: {shown}");
    }
    let shown = f
        .find(msg::NOT_RESTORED)
        .unwrap()
        .format(&msg::object(msg::NOT_RESTORED, "CUST", types::FILE, "QGPL"));
    assert_eq!(shown, "*FILE CUST was not restored to QGPL.");
}

#[test]
fn notes_fit_their_entries() {
    let f = parse(QCPFMSG).unwrap();
    for id in [
        msg::FILE_RENAMED,
        msg::OWNER_KEPT,
        msg::PGP_KEPT,
        msg::AUTL_KEPT,
        msg::AUTL_MISSING,
        msg::DEFAULT_OWNER,
        msg::JOURNAL_MISSING,
        msg::SAVF_NO_DATA,
    ] {
        let n = Note::new(id, "CUST", types::FILE, "QGPL").values("QSECOFR", "BOB");
        fits(f.find(id).unwrap(), &n.data());
    }
    let n = Note::new(msg::OWNER_KEPT, "CUST", types::FILE, "QGPL").values("QSECOFR", "BOB");
    assert_eq!(
        f.find(msg::OWNER_KEPT).unwrap().format(&n.data()),
        "*FILE CUST in QGPL keeps its owner QSECOFR."
    );
}

#[test]
fn a_restore_ends_with_cpc3703_and_its_data() {
    let f = parse(QCPFMSG).unwrap();
    let mut r = RestoreReport {
        objects: vec![object("A", "TAPLIB2", None), object("B", "TAPLIB2", None)],
        ..RestoreReport::default()
    };
    let (id, data) = msg::restore_end(&r, "TAPLIB", "TAPLIB2", 0, 1_767_225_600);
    assert_eq!(id, "CPC3703");
    assert_eq!(&data[..8], &[0, 0, 0, 2, 0, 0, 0, 0]);
    assert_eq!(ccsid::decode_trimmed(&data[8..18]), "TAPLIB");
    assert_eq!(ccsid::decode_trimmed(&data[18..28]), "TAPLIB2");
    assert_eq!(
        i64::from_be_bytes(data[28..36].try_into().unwrap()),
        1_767_225_600_000_000
    );
    let d = f.find(id).unwrap();
    fits(d, &data);
    assert_eq!(
        d.format(&data),
        "2 objects saved from library TAPLIB were restored to library TAPLIB2."
    );
    r.objects.push(object("C", "TAPLIB2", Some(msg::IN_USE)));
    let (id, data) = msg::restore_end(&r, "TAPLIB", "TAPLIB2", 0, 0);
    assert_eq!(id, "CPF3773");
    fits(f.find(id).unwrap(), &data);
    r.objects.clear();
    let (id, data) = msg::restore_end(&r, "TAPLIB", "TAPLIB2", 0, 0);
    assert_eq!(id, "CPF3770");
    fits(f.find(id).unwrap(), &data);
    r.not_found.push("X".into());
    let (id, data) = msg::restore_end(&r, "TAPLIB", "TAPLIB2", 0, 0);
    assert_eq!(id, "CPF3780");
    fits(f.find(id).unwrap(), &data);
}

#[test]
fn a_save_ends_as_its_target_says() {
    let f = parse(QCPFMSG).unwrap();
    let mut r = SaveReport {
        library: "QS71".into(),
        objects: vec![object("A", "QS71", None)],
        notes: Vec::new(),
    };
    let savf = Dest::Savf("S71".into(), "QGPL".into());
    let tape = Dest::Media {
        volumes: "VOL1 VOL2".into(),
        device: "TAPVRT01".into(),
        last: "VOL2".into(),
    };
    let (id, data) = msg::save_end(&r, &savf, 0);
    assert_eq!(id, "CPC3722");
    fits(f.find(id).unwrap(), &data);
    assert_eq!(
        f.find(id).unwrap().format(&data),
        "1 objects from library QS71 were saved to save file S71 in QGPL."
    );
    let (id, data) = msg::save_end(&r, &tape, 0);
    assert_eq!(id, "CPC3701");
    fits(f.find(id).unwrap(), &data);
    assert!(f.find(id).unwrap().format(&data).contains("VOL1 VOL2"));
    r.objects.push(object("B", "QS71", Some(msg::IN_USE)));
    for (dest, want) in [(&savf, "CPF3701"), (&tape, "CPF3771")] {
        let (id, data) = msg::save_end(&r, dest, 0);
        assert_eq!(id, want);
        fits(f.find(id).unwrap(), &data);
    }
    r.objects.remove(0);
    let (id, data) = msg::save_end(&r, &savf, 0);
    assert_eq!(id, "CPF3770");
    fits(f.find(id).unwrap(), &data);
    fits(
        f.find(msg::SAVED_LIBS_PARTIAL).unwrap(),
        &msg::libraries_end(1, 1, 0, 0),
    );
}
