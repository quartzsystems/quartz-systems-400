// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security and work management points: the shipped rows are the
//! primary's (V7R1M0) formats, the profile and password structures sit
//! at IBM's offsets, and the `PRER0100`/`PWRD0200` rounds check, then
//! execute or cancel, a refusing program's no ending them.

use regfac::formats::sec::{Password, password_parms, profile_info};
use regfac::formats::wm::{self, action};
use regfac::prepare::prepare;
use regfac::{AddOpts, ExitProgram, ExitRequest, MemRegistry, ProgramCaller, call_exit, text};

/// A caller that records each call and answers by program name.
#[derive(Default)]
struct Fake {
    calls: Vec<(String, String)>,
}

impl ProgramCaller for Fake {
    fn call(&mut self, pgm: &str, _lib: &str, parms: &mut [Vec<u8>]) -> Result<(), String> {
        let act = parms.get(2).map(|b| text::field(b)).unwrap_or_default();
        self.calls.push((pgm.to_string(), act.clone()));
        match pgm {
            // Says no on the check.
            "SAYSNO" if act == "1" => parms[0][4..8].copy_from_slice(&0i32.to_be_bytes()),
            "WAITS" if act == "1" => parms[0][8..12].copy_from_slice(&90i32.to_be_bytes()),
            "BROKEN" => return Err("CPF9810".into()),
            // A password check that refuses.
            "NOPWD" => parms[1] = text::fixed("1", 1),
            "OKPWD" => parms[1] = text::fixed("0", 1),
            _ => {}
        }
        Ok(())
    }
}

fn with(point: &str, format: &str, pgms: &[&str]) -> MemRegistry {
    let mut reg = MemRegistry::shipped();
    for (i, p) in pgms.iter().enumerate() {
        let program = ExitProgram {
            number: i32::try_from(i + 1).unwrap(),
            pgm: (*p).into(),
            lib: "QGPL".into(),
            ..ExitProgram::default()
        };
        reg.registry
            .add_program(point, format, program, &AddOpts::default())
            .unwrap_or_else(|e| panic!("{point} {format} {p}: {e}"));
    }
    reg
}

#[test]
fn the_primary_s_points_are_shipped() {
    let reg = MemRegistry::shipped().registry;
    for (p, f) in [
        ("QIBM_QSY_CHG_PROFILE", "CHGP0100"),
        ("QIBM_QSY_CHG_PROFILE", "CHGP0200"),
        ("QIBM_QSY_CHK_PASSWRD", "CHKP0100"),
        ("QIBM_QSY_CRT_PROFILE", "CRTP0100"),
        ("QIBM_QSY_DLT_PROFILE", "DLTP0100"),
        ("QIBM_QSY_DLT_PROFILE", "DLTP0200"),
        ("QIBM_QSY_RST_PROFILE", "RSTP0100"),
        ("QIBM_QSY_VLD_PASSWRD", "VLDP0100"),
        ("QIBM_QWC_PRERESTRICT", "PRER0100"),
        ("QIBM_QWC_PWRDWNSYS", "PWRD0100"),
        ("QIBM_QWC_PWRDWNSYS", "PWRD0200"),
        ("QIBM_QWT_JOBNOTIFY", "NTFY0100"),
        ("QIBM_QWT_PREATTNPGMS", "ATTN0100"),
    ] {
        assert!(reg.point(p, f).is_some(), "{p} {f}");
    }
    // 7.5 only, or REL-5's.
    assert!(reg.point("QIBM_QSY_VLD_PASSWRD", "VLDP0200").is_none());
    assert_eq!(
        reg.point("QIBM_QWT_PREATTNPGMS", "ATTN0100").unwrap().max,
        Some(8)
    );
}

#[test]
fn profile_and_password_structures() {
    let b = profile_info("QIBM_QSY_DLT_PROFILE", "DLTP0200", "ANNE");
    assert_eq!(b.len(), 38);
    assert_eq!(text::field(&b[0..20]), "QIBM_QSY_DLT_PROFILE");
    assert_eq!(text::field(&b[20..28]), "DLTP0200");
    assert_eq!(text::field(&b[28..38]), "ANNE");
    let p = password_parms(&Password {
        format: "VLDP0100".into(),
        point: "QIBM_QSY_VLD_PASSWRD".into(),
        level: 2,
        user: "ANNE".into(),
        old: None,
        new: "Secret1".into(),
    });
    let info = &p[0];
    assert_eq!(text::bin4(info, 28), 2);
    assert_eq!(text::field(&info[32..42]), "ANNE");
    // The old password is *NOPWD, both in UCS-2 after the 68 bytes.
    assert_eq!(text::bin4(info, 44), 68);
    assert_eq!(text::bin4(info, 48), 12);
    assert_eq!(text::bin4(info, 52), 13488);
    assert_eq!(text::bin4(info, 56), 80);
    assert_eq!(text::bin4(info, 60), 14);
    assert_eq!(info.len(), 94);
    assert_eq!(p[1], vec![0x40], "the answer starts blank: only 0 accepts");
}

#[test]
fn a_password_program_that_refuses_ends_the_calls() {
    let req = ExitRequest::Password(Password {
        format: "VLDP0100".into(),
        point: "QIBM_QSY_VLD_PASSWRD".into(),
        user: "ANNE".into(),
        new: "x".into(),
        ..Password::default()
    });
    let mut reg = with(
        "QIBM_QSY_VLD_PASSWRD",
        "VLDP0100",
        &["OKPWD", "NOPWD", "OKPWD"],
    );
    let mut c = Fake::default();
    let a = call_exit(&mut reg, &mut c, "QIBM_QSY_VLD_PASSWRD", &req);
    assert!(!a.allow);
    assert_eq!(c.calls.len(), 2);
    let mut reg = with("QIBM_QSY_VLD_PASSWRD", "VLDP0100", &["BROKEN"]);
    assert!(!call_exit(&mut reg, &mut Fake::default(), "QIBM_QSY_VLD_PASSWRD", &req).allow);
}

#[test]
fn a_notification_ignores_a_broken_program() {
    let req = ExitRequest::Profile {
        point: "QIBM_QSY_CRT_PROFILE".into(),
        format: "CRTP0100".into(),
        user: "ANNE".into(),
    };
    let mut reg = with("QIBM_QSY_CRT_PROFILE", "CRTP0100", &["BROKEN", "LOGS"]);
    let mut c = Fake::default();
    assert!(call_exit(&mut reg, &mut c, "QIBM_QSY_CRT_PROFILE", &req).allow);
    assert_eq!(c.calls.len(), 2, "every program is called");
}

#[test]
fn prerestrict_checks_then_executes_or_cancels() {
    let mut reg = with("QIBM_QWC_PRERESTRICT", "PRER0100", &["FIRST", "WAITS"]);
    let mut c = Fake::default();
    let r = prepare(
        &mut reg,
        &mut c,
        "QIBM_QWC_PRERESTRICT",
        "PRER0100",
        wm::delay::IMMED,
    );
    assert!(r.allowed);
    assert_eq!(r.wait, 90);
    let acts: Vec<&str> = c.calls.iter().map(|(_, a)| a.as_str()).collect();
    assert_eq!(acts, ["1", "1", "2", "2"]);

    let mut reg = with(
        "QIBM_QWC_PRERESTRICT",
        "PRER0100",
        &["FIRST", "SAYSNO", "LAST"],
    );
    let mut c = Fake::default();
    let r = prepare(&mut reg, &mut c, "QIBM_QWC_PRERESTRICT", "PRER0100", 60);
    assert!(!r.allowed);
    assert_eq!(r.refused_by.as_deref(), Some("QGPL/SAYSNO"));
    let seen: Vec<(String, String)> = c.calls.clone();
    assert_eq!(
        seen,
        [
            ("FIRST".into(), "1".into()),
            ("SAYSNO".into(), "1".into()),
            ("FIRST".into(), "3".into())
        ],
        "the one that said no and the ones never asked are not told to cancel"
    );
    // A program that cannot be called counts as yes.
    let mut reg = with("QIBM_QWC_PWRDWNSYS", "PWRD0200", &["BROKEN"]);
    assert!(
        prepare(
            &mut reg,
            &mut Fake::default(),
            "QIBM_QWC_PWRDWNSYS",
            "PWRD0200",
            -1
        )
        .allowed
    );
    assert_eq!(action::CHECK, '1');
}
