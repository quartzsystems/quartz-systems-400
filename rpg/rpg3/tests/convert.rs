// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The converter on small members (the corpus checks it against IBM's,
//! `tools/conformance/tests/rpg3_cvt.rs`).

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

#[test]
fn moves_definitions_and_renames_operations() {
    let src = lines(concat!(
        "     H\n",
        "     FQSYSPRT O   F     132     OF     PRINTER\n",
        "     E                    ARR     4   8  3 0\n",
        "     C           ARR,1     ADD  1         X       30\n",
        "     C                     SETOF                     LR\n",
        "     C   01 02N03          EXCPTLINE\n",
        "     OQSYSPRT E  1             LINE\n",
        "     O                         X     Z   10\n",
    ));
    let c = rpg3::convert(&src);
    let t = c.text();
    assert_eq!(t[1], "     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)");
    assert_eq!(t[2], "     D ARR             S              3  0 DIM(8) CTDATA PERRCD(4)");
    assert_eq!(t[3], "     C     ARR(1)        ADD       1             X                 3 0");
    assert_eq!(t[4], "     C                   SETOFF                                       LR");
    assert_eq!(t[5], "     C   01");
    assert_eq!(t[6], "     CAN 02");
    assert_eq!(t[7], "     CANN03              EXCEPT    LINE");
    assert_eq!(t[8], "     OQSYSPRT   E            LINE           1");
    assert_eq!(t[9], "     O                       X             Z     10");
    // Each line knows its RPG III record.
    assert_eq!(c.lines[7].from, 5);
    assert!(c.messages.is_empty());
}
