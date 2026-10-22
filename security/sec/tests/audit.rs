// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Audit entries in `QSYS/QAUDJRN`: written, read back, and refused
//! by a full receiver.

mod common;

use common::{NOW, open_new};
use qsname::Name;
use sec::audit::write::{self, Outcome};
use sec::audit::{Event, Kind};

fn job() -> jrn::JobInfo {
    jrn::JobInfo {
        name: "QPADEV0001".into(),
        user: "BOB".into(),
        number: 42,
    }
}

#[test]
fn write_read_full() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let ev = Event::new(Kind::Af)
        .set("VIOL", "A")
        .object("PAYROLL", "APPLIB", "*FILE");
    assert_eq!(
        write::write(&mut t, qsys, &[(ev.clone(), job())]).unwrap(),
        Outcome::NoJournal
    );
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        qsys,
        Name::parse("AUDRCV0001").unwrap(),
        &jrn::rcv::CrtJrnRcvOpts {
            threshold_kb: 1,
            now: NOW,
            ..jrn::rcv::CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    jrn::journal::crtjrn(
        &mut t,
        qsys,
        Name::parse("QAUDJRN").unwrap(),
        rcv,
        &jrn::CrtJrnOpts {
            mngrcv_system: false,
            now: NOW,
            ..jrn::CrtJrnOpts::default()
        },
    )
    .unwrap();
    let cp = Event::new(Kind::Cp)
        .set("CMDN", "CRTUSRPRF")
        .set("USRP", "EVE");
    assert_eq!(
        write::write(&mut t, qsys, &[(ev, job()), (cp, job())]).unwrap(),
        Outcome::Written(2)
    );
    let back = write::entries(&mut t, qsys, &jrn::Filter::default()).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(&back[0].0.kind, b"AF");
    assert_eq!(back[0].0.job.user, "BOB");
    assert_eq!(back[0].1.get("ONAM"), "PAYROLL");
    assert_eq!(back[1].1.get("USRP"), "EVE");
    // A one-kilobyte threshold under MNGRCV(*USER): soon full.
    let mut full = false;
    for _ in 0..100 {
        let e = Event::new(Kind::Sv)
            .set("SYSV", "QMAXSIGN")
            .set("NVAL", "5");
        if write::write(&mut t, qsys, &[(e, job())]).unwrap() == Outcome::Full {
            full = true;
            break;
        }
    }
    assert!(full);
}
