// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Rules pub400 (V7R5M0) settled for CRTDSPF, checked on 2026-11-08:
//! `SFLRCDNBR` takes no option indicators (`CPD7488`), and a subfile
//! control record in a window with fields above and below its
//! subfile's lines overlaps them (`CPD7812`).

use dds::{CompileOpts, NoRefs};

fn ids(lines: &[&str]) -> Vec<&'static str> {
    let (src, _) = dds::Source::from_text(&lines.join("\n"));
    dspf::compile_dspf(&src, &NoRefs, &CompileOpts::default())
        .diagnostics
        .iter()
        .map(|d| d.id)
        .collect()
}

#[test]
fn sflrcdnbr_takes_no_option_indicators() {
    let src = [
        "     A                                      DSPSIZ(24 80 *DS3)",
        "     A          R SFL1                      SFL",
        "     A            SNUM           3Y 0O  6  2EDTCDE(Z)",
        "     A          R CTL1                      SFLCTL(SFL1)",
        "     A                                      SFLSIZ(0020)",
        "     A                                      SFLPAG(0004)",
        "     A  31                                  SFLDSP",
        "     A                                      SFLDSPCTL",
        "     A                                  1  2'Title'",
        "     A            RCD            4S 0H",
        "     A  35                                  SFLRCDNBR(CURSOR)",
    ];
    assert!(ids(&src).contains(&"DDS6207"), "{:?}", ids(&src));
    // Unconditioned it is fine.
    let mut plain = src;
    plain[10] = "     A                                      SFLRCDNBR(CURSOR)";
    assert!(!ids(&plain).contains(&"DDS6207"), "{:?}", ids(&plain));
}

#[test]
fn a_window_control_record_may_not_straddle_its_subfile() {
    let src = |below: bool| {
        let mut l = vec![
            "     A                                      DSPSIZ(24 80 *DS3)",
            "     A          R WSFL                      SFL",
            "     A            WNUM           3Y 0O  6  2EDTCDE(Z)",
            "     A          R WCTL                      SFLCTL(WSFL)",
            "     A                                      SFLSIZ(0012)",
            "     A                                      SFLPAG(0006)",
            "     A                                      WINDOW(2 10 14 40)",
            "     A                                      OVERLAY",
            "     A  31                                  SFLDSP",
            "     A                                      SFLDSPCTL",
            "     A  34                                  SFLEND(*MORE)",
            "     A                                  1  2'Title'",
            "     A                                  5  2'Num'",
        ];
        if below {
            l.push("     A                                 12  2'F3=Exit'");
        }
        ids(&l)
    };
    assert!(src(true).contains(&"DDS6520"), "{:?}", src(true));
    assert!(!src(false).contains(&"DDS6520"), "{:?}", src(false));
}
