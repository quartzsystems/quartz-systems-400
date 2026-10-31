// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LIC PTFs over a fake ESP and firmware: stage into the other slot,
//! the IPL that boots it, a power loss before it, removal by an IPL
//! back to the old slot, permanent apply into both slots.

use std::io;

use ptf::db::PtfRecord;
use ptf::slot::{self, Esp, Firmware, IplEvent, Slot, SlotState};
use ptf::{Action, PtfDb, Status};

#[derive(Default)]
struct MemEsp {
    a: Vec<u8>,
    b: Vec<u8>,
    /// Fail the next write halfway (a power loss).
    tear: bool,
}

impl Esp for MemEsp {
    fn write(&mut self, s: Slot, image: &[u8]) -> io::Result<()> {
        let dst = match s {
            Slot::A => &mut self.a,
            Slot::B => &mut self.b,
        };
        if self.tear {
            self.tear = false;
            *dst = image[..image.len() / 2].to_vec();
            return Err(io::Error::other("power lost"));
        }
        *dst = image.to_vec();
        Ok(())
    }
}

/// The firmware: a one-shot next and a default.
#[derive(Default)]
struct Fw {
    next: Option<Slot>,
    default: Option<Slot>,
}

impl Firmware for Fw {
    fn boot_next(&mut self, s: Slot) -> Result<(), String> {
        self.next = Some(s);
        Ok(())
    }
    fn boot_default(&mut self, s: Slot) -> Result<(), String> {
        self.default = Some(s);
        Ok(())
    }
}

impl Fw {
    /// Power on: the one-shot is consumed whether or not the IPL ends.
    fn boot(&mut self) -> Slot {
        self.next.take().or(self.default).unwrap_or(Slot::A)
    }
}

fn db() -> PtfDb {
    let mut db = PtfDb::default();
    db.ptfs.insert(
        "QS00050".into(),
        PtfRecord {
            ptf: "QS00050".into(),
            product: "5770SS1".into(),
            option: "0000".into(),
            release: "V1R0M0".into(),
            status: Status::Loaded,
            action: Action::ApplyAtIpl,
            delayed: true,
            lic: true,
            savf: "QGPL/QS00050".into(),
            superseded_by: String::new(),
            prereqs: Vec::new(),
            coreqs: Vec::new(),
            replaced: Vec::new(),
            changed: 0,
        },
    );
    db
}

#[test]
fn apply_through_the_other_slot_and_remove() {
    let mut esp = MemEsp {
        a: b"old".to_vec(),
        b: b"old-b".to_vec(),
        tear: false,
    };
    let mut fw = Fw::default();
    let mut st = SlotState::default();
    let mut db = db();
    let booted = fw.boot();
    assert_eq!(booted, Slot::A);
    // APYPTF: the image goes to B.
    let t = slot::stage(&mut st, &mut esp, booted, "QS00050", b"new", Some(b"new-b")).unwrap();
    assert_eq!(t, Slot::B);
    assert_eq!(esp.b, b"new-b");
    assert_eq!(esp.a, b"old", "the booted slot is untouched");
    // PWRDWNSYS RESTART(*YES): arm B once.
    fw.boot_next(slot::next_slot(&st, None).unwrap()).unwrap();
    // A power loss during that IPL: the firmware already consumed it.
    assert_eq!(fw.boot(), Slot::B);
    // ... and the next power-on boots the old slot.
    let booted = fw.boot();
    assert_eq!(booted, Slot::A);
    let (ev, _) = slot::at_ipl(&mut st, &mut db, booted, &mut fw, 1);
    assert_eq!(ev, [IplEvent::Waiting("QS00050".into())]);
    assert_eq!(db.get("QS00050").unwrap().action, Action::ApplyAtIpl);
    // The next power down tries again; this IPL comes up.
    fw.boot_next(slot::next_slot(&st, None).unwrap()).unwrap();
    let booted = fw.boot();
    let (ev, _) = slot::at_ipl(&mut st, &mut db, booted, &mut fw, 2);
    assert_eq!(ev, [IplEvent::Applied("QS00050".into())]);
    assert_eq!(db.get("QS00050").unwrap().status, Status::TempApplied);
    assert_eq!(fw.default, Some(Slot::B), "B is the IPL source from now on");
    assert_eq!(fw.boot(), Slot::B);
    // The state survives the LIC record.
    assert_eq!(SlotState::decode(&st.encode()), st);
    // A second LIC PTF cannot overwrite the old image while this one is temporary.
    assert!(slot::stage(&mut st, &mut esp, Slot::B, "QS00051", b"x", None).is_err());
    // RMVPTF: an IPL back to A.
    let back = slot::remove(&mut st, &mut db, "QS00050", 3).unwrap();
    assert_eq!(back, Slot::A);
    assert_eq!(db.get("QS00050").unwrap().action, Action::RemoveAtIpl);
    fw.boot_next(slot::next_slot(&st, None).unwrap()).unwrap();
    let booted = fw.boot();
    let (ev, _) = slot::at_ipl(&mut st, &mut db, booted, &mut fw, 4);
    assert_eq!(ev, [IplEvent::Removed("QS00050".into())]);
    assert_eq!(db.get("QS00050").unwrap().status, Status::TempRemoved);
    assert_eq!(fw.default, Some(Slot::A));
    assert!(st.temp.is_empty());
}

#[test]
fn permanent_apply_writes_both_slots() {
    let mut esp = MemEsp::default();
    let mut fw = Fw::default();
    let mut st = SlotState::default();
    let mut db = db();
    slot::stage(&mut st, &mut esp, Slot::A, "QS00050", b"new", Some(b"new-b")).unwrap();
    let (ev, _) = slot::at_ipl(&mut st, &mut db, Slot::B, &mut fw, 1);
    assert_eq!(ev, [IplEvent::Applied("QS00050".into())]);
    slot::make_permanent(&mut st, &mut esp, &mut db, "QS00050", b"new", Some(b"new-b"), 2).unwrap();
    assert_eq!((esp.a.as_slice(), esp.b.as_slice()), (&b"new"[..], &b"new-b"[..]));
    assert_eq!(db.get("QS00050").unwrap().status, Status::PermApplied);
    // Now a further LIC PTF may use the other slot again.
    assert!(st.temp.is_empty());
    assert_eq!(slot::stage(&mut st, &mut esp, Slot::B, "QS00051", b"x", None).unwrap(), Slot::A);
}

#[test]
fn a_torn_stage_is_written_again() {
    let mut esp = MemEsp {
        tear: true,
        ..MemEsp::default()
    };
    let mut fw = Fw::default();
    let mut st = SlotState::default();
    let mut db = db();
    assert!(slot::stage(&mut st, &mut esp, Slot::A, "QS00050", b"new image", None).is_err());
    assert!(st.pending.is_none(), "nothing armed for a torn image");
    assert_eq!(slot::next_slot(&st, None), None);
    let (ev, _) = slot::at_ipl(&mut st, &mut db, Slot::A, &mut fw, 1);
    assert_eq!(ev, [IplEvent::Restage("QS00050".into())]);
    slot::stage(&mut st, &mut esp, Slot::A, "QS00050", b"new image", None).unwrap();
    assert_eq!(esp.b, b"new image");
    // An explicit IPLSRC wins over the pending slot.
    assert_eq!(slot::next_slot(&st, Some(Slot::A)), Some(Slot::A));
}
