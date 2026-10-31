// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZRTVPR receivers, byte for byte, against `docs/LICPGM.md` §4.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{Want, e, prdi, seeded, sys};
use licpgm::api::rtvpr::qszrtvpr;
use licpgm::{LoadState, ProductStore};

/// `PRDR0100` of the operating system's code load.
fn os_prdr0100(echo: &str) -> Want {
    let mut w = Want::default();
    w.bin(0, 108)
        .bin(4, 108)
        .bin(8, 0)
        .ch(12, 7, echo)
        .ch(19, 6, "V1R0M0")
        .ch(25, 4, "0000")
        .ch(29, 4, "5050")
        .ch(33, 10, "*CODE")
        .ch(43, 10, "*INSTALLED")
        .ch(53, 10, "*NONE")
        .ch(63, 2, "90")
        .ch(65, 1, "1")
        .ch(66, 2, "04")
        .ch(68, 14, "QS40001")
        .ch(82, 2, "")
        .bin(84, 0)
        .ch(88, 4, "2924")
        .ch(92, 6, "V1R0M0")
        .ch(98, 6, "V1R0M0")
        .ch(104, 1, "1")
        .ch(105, 3, "L00");
    w
}

#[test]
fn prdr0100_of_the_operating_system() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        200,
        "PRDR0100",
        &prdi("*OPSYS", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    assert_eq!(got, os_prdr0100("QS00SS1").0);
    // IBM's ID finds the product and is echoed.
    let got = qszrtvpr(
        &s,
        &sys(),
        200,
        "PRDR0100",
        &prdi("5770SS1", "V1R0M0", "0000", "*CODE"),
        None,
    )
    .unwrap();
    assert_eq!(got, os_prdr0100("5770SS1").0);
    // A short receiver: bytes returned is what fits.
    let got = qszrtvpr(
        &s,
        &sys(),
        50,
        "PRDR0100",
        &prdi("*OPSYS", "*ONLY", "", ""),
        None,
    )
    .unwrap();
    let mut want = os_prdr0100("QS00SS1");
    want.bin(0, 50);
    assert_eq!(got, want.0[..50]);
}

#[test]
fn prdr0100_of_a_language_load_and_an_option() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        200,
        "PRDR0100",
        &prdi("QS00WDS", "V1R0M0", "0031", "2924"),
        None,
    )
    .unwrap();
    let mut w = Want::default();
    w.bin(0, 108)
        .bin(4, 108)
        .ch(12, 7, "QS00WDS")
        .ch(19, 6, "V1R0M0")
        .ch(25, 4, "0031")
        .ch(29, 4, "2924")
        .ch(33, 10, "*LNG")
        .ch(43, 10, "*INSTALLED")
        .ch(53, 10, "*NONE")
        .ch(63, 2, "90")
        .ch(65, 1, "1")
        .ch(66, 2, "04")
        .ch(68, 14, "QS40001")
        .ch(82, 2, "")
        .bin(84, 0)
        .ch(88, 4, "2924")
        .ch(92, 6, "V1R0M0")
        .ch(98, 6, "V1R0M0")
        .ch(104, 1, "0")
        .ch(105, 3, "");
    assert_eq!(got, w.0);
}

#[test]
fn prdr0100_of_a_reserved_product_is_defined() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        200,
        "PRDR0100",
        &prdi("QS00DG1", "V1R0M0", "0000", "*CODE"),
        None,
    )
    .unwrap();
    assert_eq!(&got[43..53], e("*DEFINED", 10).as_slice());
    assert_eq!(&got[63..65], e("10", 2).as_slice());
    assert_eq!(&got[104..105], e("0", 1).as_slice());
}

#[test]
fn prdr0200_libraries() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0200",
        &prdi("*OPSYS", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    let mut w = os_prdr0100("QS00SS1");
    w.bin(84, 108)
        .ch(108, 10, "")
        .ch(118, 2, "")
        .bin(120, 4)
        .bin(124, 128);
    for (i, lib) in ["QSYS", "QSYS2", "QUSRSYS", "QGPL"].iter().enumerate() {
        let at = 128 + 68 * i;
        let next = if i == 3 {
            0
        } else {
            i32::try_from(at + 68).unwrap()
        };
        w.bin(at, next)
            .ch(at + 4, 10, lib)
            .ch(at + 14, 10, lib)
            .ch(at + 24, 10, "*PROD")
            .ch(at + 34, 10, "*USE")
            .ch(at + 44, 10, "*SYSVAL")
            .ch(at + 54, 10, "*NONE")
            .bin(at + 64, 0);
    }
    w.bin(0, 400).bin(4, 400);
    assert_eq!(got, w.0);
}

#[test]
fn prdr0300_and_0400() {
    let mut s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0300",
        &prdi("*OPSYS", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    let mut w = os_prdr0100("QS00SS1");
    w.bin(0, 120)
        .bin(4, 120)
        .bin(84, 108)
        .bin(108, 0)
        .bin(112, 126)
        .bin(116, 120);
    assert_eq!(got, w.0);

    // A vendor product with objects, installed.
    s.libs.insert("INVPRD".into());
    let pkg = common::vendor();
    s.create_definition(&pkg.definition_name, &pkg.definition)
        .unwrap();
    for (q, l) in &pkg.loads {
        let mut l = l.clone();
        l.state = LoadState::Installed;
        s.create_load(q, &l).unwrap();
    }
    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0400",
        &prdi("1AB1234", "*ONLY", "0000", "*CODE"),
        None,
    )
    .unwrap();
    assert_eq!(&got[12..19], e("1AB1234", 7).as_slice());
    assert_eq!(&got[66..68], e("02", 2).as_slice());
    assert_eq!(&got[68..82], e("15551234", 14).as_slice());
    assert_eq!(&got[105..108], e("", 3).as_slice());
    let mut w = Want(got[..108].to_vec());
    w.bin(0, 180)
        .bin(4, 180)
        .bin(108, 2)
        .bin(112, 30)
        .bin(116, 120)
        .ch(120, 10, "INVMAIN")
        .ch(130, 10, "INVPRD")
        .ch(140, 10, "*PGM")
        .ch(150, 10, "INVFILE")
        .ch(160, 10, "INVDTA")
        .ch(170, 10, "*FILE");
    assert_eq!(got, w.0);
}

#[test]
fn prdr0500_definition() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0500",
        &prdi("QS00WDS", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    let mut w = Want(got[..108].to_vec());
    w.bin(84, 108)
        .ch(108, 1, "0")
        .ch(109, 1, "")
        .ch(110, 6, "")
        .ch(116, 4, "2026")
        .ch(120, 4, "2026")
        .ch(124, 10, "")
        .ch(134, 10, "")
        .bin(144, 4)
        .bin(148, 18)
        .bin(152, 160)
        .ch(156, 1, "0")
        .ch(157, 3, "");
    for (i, (o, m)) in [
        ("0000", "QSZ1000"),
        ("0021", "QSZ1021"),
        ("0031", "QSZ1031"),
        ("0061", "QSZ1061"),
    ]
    .iter()
    .enumerate()
    {
        let at = 160 + 18 * i;
        w.ch(at, 4, o)
            .ch(at + 4, 1, "0")
            .ch(at + 5, 7, m)
            .ch(at + 12, 6, "V1R0M0");
    }
    w.bin(0, 232).bin(4, 232);
    assert_eq!(got, w.0);
}

#[test]
fn prdr0600_loads_and_0700_releases() {
    let s = seeded();
    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0600",
        &prdi("QS00TC1", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    let mut w = Want(got[..108].to_vec());
    w.bin(0, 136)
        .bin(4, 136)
        .bin(84, 108)
        .bin(108, 2)
        .bin(112, 8)
        .bin(116, 120)
        .ch(120, 4, "0000")
        .ch(124, 4, "5001")
        .ch(128, 4, "0000")
        .ch(132, 4, "2924");
    assert_eq!(got, w.0);

    let got = qszrtvpr(
        &s,
        &sys(),
        1000,
        "PRDR0700",
        &prdi("QS00TC1", "*CUR", "0000", "*CODE"),
        None,
    )
    .unwrap();
    let mut w = Want::default();
    w.bin(0, 22)
        .bin(4, 22)
        .bin(8, 1)
        .bin(12, 0)
        .ch(16, 6, "V1R0M0");
    assert_eq!(got, w.0);
}

#[test]
fn prdr0800_directories_in_utf8() {
    let s = seeded();
    let mut info = prdi("QS00SS1", "*CUR", "0030", "*CODE");
    info.push(0x40);
    info.extend(36i32.to_be_bytes());
    info.extend(1208i32.to_be_bytes());
    let got = qszrtvpr(&s, &sys(), 1000, "PRDR0800", &info, Some("PRDI0200")).unwrap();
    let path = b"/QIBM/ProdData/qsh";
    let mut w = Want(got[..108].to_vec());
    let data = 128 + 52;
    w.bin(84, 108)
        .bin(108, 1)
        .bin(112, 52)
        .bin(116, 128)
        .bin(120, 1208)
        .ch(124, 1, "0")
        .ch(125, 3, "")
        .bin(128, 18)
        .bin(132, i32::try_from(data).unwrap())
        .bin(136, 3)
        .bin(140, i32::try_from(data + 18).unwrap())
        .bin(144, 18)
        .bin(148, i32::try_from(data + 21).unwrap())
        .bin(152, 3)
        .bin(156, i32::try_from(data + 39).unwrap())
        .bin(160, 0)
        .bin(164, 0)
        .ch(168, 10, "*RX")
        .ch(178, 2, "")
        .at(data, path)
        .at(data + 18, b"qsh")
        .at(data + 21, path)
        .at(data + 39, b"qsh");
    let n = i32::try_from(w.0.len()).unwrap();
    w.bin(0, n).bin(4, n);
    assert_eq!(got, w.0);
}

#[test]
fn errors() {
    let s = seeded();
    let info = prdi("*OPSYS", "*CUR", "0000", "*CODE");
    let err = |r: Result<Vec<u8>, licpgm::api::ApiError>| r.unwrap_err().id;
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 200, "PRDR0999", &info, None)),
        "CPF3C21"
    );
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 4, "PRDR0100", &info, None)),
        "CPF3C24"
    );
    assert_eq!(
        err(qszrtvpr(
            &s,
            &sys(),
            200,
            "PRDR0100",
            &info,
            Some("PRDI0900")
        )),
        "CPF3C21"
    );
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 200, "PRDR0100", &info[..20], None)),
        "CPF3C1D"
    );
    let none = prdi("9XX9999", "*CUR", "0000", "*CODE");
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 200, "PRDR0100", &none, None)),
        "CPF0C1F"
    );
    let bad_opt = prdi("*OPSYS", "V1R0M0", "0099", "*CODE");
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 200, "PRDR0100", &bad_opt, None)),
        "CPF0C1F"
    );
    let bad_rel = prdi("*OPSYS", "V9", "0000", "*CODE");
    assert_eq!(
        err(qszrtvpr(&s, &sys(), 200, "PRDR0100", &bad_rel, None)),
        "CPF0C1D"
    );
}
