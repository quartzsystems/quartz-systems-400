// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QUSRTVEI` receivers: `EXTI0300` with descriptions and data,
//! selection on the program data, and a continuation handle that picks
//! up where a small receiver stopped.

use regfac::exti::{self, Select};
use regfac::{AddOpts, Desc, ExitPoint, ExitProgram, Registry};

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

fn registry() -> Registry {
    let mut r = Registry::shipped();
    for (n, data) in [(1, "ALPHA"), (2, "BETA"), (3, "ALPHA2")] {
        r.add_program(
            "QIBM_QNPS_ENTRY",
            "ENTR0100",
            ExitProgram {
                number: n,
                pgm: format!("PGM{n}"),
                lib: "QSTEST".into(),
                data: regfac::text::ebcdic(data),
                desc: Desc::text(&format!("Program {n}")),
                ..ExitProgram::default()
            },
            &AddOpts::default(),
        )
        .unwrap();
    }
    r
}

#[test]
fn exti0300_carries_descriptions_and_data() {
    let r = registry();
    let p: Vec<&ExitPoint> = r.select("QIBM_QNPS_ENTRY", "ENTR0100", regfac::Which::All);
    let b = exti::exti0200(&p, -1, &[], true, 0, 4096);
    assert_eq!(bin4(&b, 28), 3);
    assert_eq!(bin4(&b, 32), 156);
    let mut at = usize::try_from(bin4(&b, 24)).unwrap();
    for n in 1..=3 {
        let e = &b[at..];
        assert_eq!(bin4(e, 36), n);
        assert_eq!(chars(e, 40, 10), format!("PGM{n}"));
        assert_eq!(chars(e, 60, 1), "T");
        assert_eq!(chars(e, 88, 50), format!("Program {n}"));
        let off = usize::try_from(bin4(e, 144)).unwrap();
        let len = usize::try_from(bin4(e, 148)).unwrap();
        assert!(chars(&b, off, len).starts_with(if n == 2 { "BETA" } else { "ALPHA" }));
        at = usize::try_from(bin4(e, 0)).unwrap();
    }
    assert_eq!(at, b.len(), "the last entry's next offset is the end");
}

#[test]
fn selection_compares_program_data() {
    let r = registry();
    let p = r.select("QIBM_QNPS_ENTRY", "ENTR0100", regfac::Which::All);
    let sel = [Select {
        op: 1,
        start: 1,
        data: regfac::text::ebcdic("ALPHA"),
    }];
    let b = exti::exti0200(&p, -1, &sel, false, 0, 4096);
    assert_eq!(bin4(&b, 28), 2, "ALPHA and ALPHA2");
    // The parameter form parses back.
    let mut raw = 1i32.to_be_bytes().to_vec();
    raw.extend(21i32.to_be_bytes());
    raw.extend(1i32.to_be_bytes());
    raw.extend(1i32.to_be_bytes());
    raw.extend(5i32.to_be_bytes());
    raw.extend(regfac::text::ebcdic("ALPHA"));
    assert_eq!(exti::parse_selection(&raw).unwrap(), sel.to_vec());
    raw[8..12].copy_from_slice(&2i32.to_be_bytes());
    assert_eq!(exti::parse_selection(&raw).unwrap_err().msgid, "CPF3CE4");
}

#[test]
fn a_small_receiver_continues() {
    let r = Registry::shipped();
    let all = r.select("*ALL", "*ALL", regfac::Which::All);
    let room = 36 + 204 * 5;
    let first = exti::exti0100(&all, 0, room);
    assert_eq!(bin4(&first, 28), 5);
    assert_eq!(bin4(&first, 0), i32::try_from(room).unwrap());
    let total = bin4(&first, 4);
    assert_eq!(total, i32::try_from(36 + 204 * all.len()).unwrap());
    let at = exti::resume_at(&first[8..24]).unwrap();
    assert_eq!(at, 5);
    let next = exti::exti0100(&all, at, room);
    assert_eq!(chars(&next, 36, 20), all[5].name);
    assert_eq!(exti::resume_at(&[0x40; 16]).unwrap(), 0);
    assert_eq!(
        exti::resume_at(&regfac::text::fixed("NOT A HANDLE", 16))
            .unwrap_err()
            .msgid,
        "CPF3CE2"
    );
}
