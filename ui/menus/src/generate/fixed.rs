// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MAJOR`: the fixed groups, as the goldens `menu-major.txt` and
//! `menu-major-2.txt` number them. Menu titles and option texts are
//! the screen interface; the groups whose contents IBM curates by hand
//! are `member::HAND_WRITTEN`.

/// `(text, name at column 70, action)` in option order, 1 to 23.
pub const MAJOR: &[(&str, &str, &str)] = &[
    ("Select Command by Name", "SLTCMD", "CMD SLTCMD"),
    ("Verb Commands", "VERB", "MENU VERB"),
    ("Subject Commands", "SUBJECT", "MENU SUBJECT"),
    ("Object Management Commands", "CMDOBJMGT", "MENU CMDOBJMGT"),
    ("File Commands", "CMDFILE", "MENU CMDFILE"),
    ("Save and Restore Commands", "CMDSAVRST", "MENU CMDSAVRST"),
    ("Work Management Commands", "CMDWRKMGT", "MENU CMDWRKMGT"),
    ("Data Management Commands", "CMDDTAMGT", "MENU CMDDTAMGT"),
    ("Security Commands", "CMDSEC", "MENU CMDSEC"),
    ("Print Commands", "CMDPRT", "MENU CMDPRT"),
    ("Spooling Commands", "CMDSPL", "MENU CMDSPL"),
    ("System Control Commands", "CMDSYSCTL", "MENU CMDSYSCTL"),
    ("Program Commands", "CMDPGM", "MENU CMDPGM"),
    ("Office Commands", "CMDOFC", "MENU CMDOFC"),
    ("Database Commands", "CMDDB", "MENU CMDDB"),
    (
        "Communications Management Commands",
        "CMDCMNMGT",
        "MENU CMDCMNMGT",
    ),
    (
        "Distribution Services Commands",
        "CMDDSTSRV",
        "MENU CMDDSTSRV",
    ),
    ("Configuration Commands", "CMDCFG", "MENU CMDCFG"),
    ("Problem Management Commands", "CMDPRBMGT", "MENU CMDPRBMGT"),
    ("Message Handling Commands", "CMDMSGHDL", "MENU CMDMSGHDL"),
    ("Performance Commands", "CMDPFR", "MENU CMDPFR"),
    ("System/36 Commands", "CMDS36", "MENU CMDS36"),
    ("System/38 Commands", "CMDS38", "MENU CMDS38"),
];

/// The titles of the three fixed menus.
pub const MAJOR_TITLE: &str = "Major Command Groups";
/// `VERB`'s title.
pub const VERB_TITLE: &str = "Verb Commands";
/// `SUBJECT`'s title.
pub const SUBJECT_TITLE: &str = "Subject Commands";
