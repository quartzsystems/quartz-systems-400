// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The menus and commands IBM's menus name that this release does not
//! have: an option leading to one answers with a message instead of an
//! object that is not found (`docs/MENUS.md` §7 holds the table with
//! the reason for each).

/// `CPD9D60`: the command is not available in this release.
pub const COMMAND: &str = "CPD9D60";
/// `CPD9D61`: the menu is not available in this release.
pub const MENU: &str = "CPD9D61";

/// Menus left out, by name.
pub const MENUS: &[&str] = &[
    "CFGPCS",
    "CMDOFC",
    "CMDS36",
    "CMDS38",
    "CMNCFG",
    "CRQADM",
    "DECISION",
    "DISKETTE",
    "FILETFR",
    "FOLDER",
    "IDDU",
    "NWSADM",
    "OFCSEC",
    "OPTSERVER",
    "PERFORM",
    "PROGRAMS36",
    "QUERY",
    "RSTS36FMT",
    "SAVS36FMT",
    "SECTOOLS",
    "SVSM",
    "UTILITIES",
];

/// Commands left out, by name.
pub const COMMANDS: &[&str] = &[
    "ASKQST",
    "CFGDSTSRV",
    "CFGRPDS",
    "CHGPRF",
    "CHKOPTVOL",
    "CPYOPT",
    "CPYPTFCVR",
    "CPYPTFGRP",
    "CVTOPTBKU",
    "DLTPTF",
    "DSPAPPNINF",
    "DSPDSTLOG",
    "DSPDSTSRV",
    "DSPPWRSCD",
    "DSPSOCSTS",
    "EDTRBDAP",
    "ENDCPYSCN",
    "ENDRMTSPT",
    "PRTDEVADR",
    "RCLOPT",
    "RSTDLO",
    "RSTS36F",
    "RSTS36LIBM",
    "RTVCFGSRC",
    "SAVAPARDTA",
    "SAVDLO",
    "SAVS36F",
    "SAVS36LIBM",
    "SBMDBJOB",
    "SLTCMD",
    "SNDNETMSG",
    "STRCPYSCN",
    "STRCSP",
    "STREDU",
    "STRPASTHR",
    "STRPGMMNU",
    "STRRMTSPT",
    "STRSCHIDX",
    "STRTCPTELN",
    "VFYAPPCCNN",
    "VFYCMN",
    "VFYTAP",
    "WRKALR",
    "WRKALRD",
    "WRKCFGL",
    "WRKCNNL",
    "WRKCOSD",
    "WRKCTLD",
    "WRKDDMF",
    "WRKDIRE",
    "WRKDOC",
    "WRKDOCLIB",
    "WRKDSTQ",
    "WRKFLR",
    "WRKHLDOPTF",
    "WRKMODD",
    "WRKNETJOBE",
    "WRKNTBD",
    "WRKNWID",
    "WRKNWSCFG",
    "WRKNWSD",
    "WRKOPTDIR",
    "WRKOPTF",
    "WRKSOC",
];

/// The message for a menu option's `MENU name`, when it is left out.
#[must_use]
pub fn menu(name: &str) -> Option<&'static str> {
    let name = name.trim().to_ascii_uppercase();
    MENUS.contains(&name.as_str()).then_some(MENU)
}

/// The message for a command string whose command is left out: the
/// first word, past a `?` and a library.
#[must_use]
pub fn command(text: &str) -> Option<(&'static str, String)> {
    let name = command_name(text);
    COMMANDS.contains(&name.as_str()).then_some((COMMAND, name))
}

/// The command a command string names, upper case: the first word past
/// a `?`, before a `(`, without its library.
#[must_use]
pub fn command_name(text: &str) -> String {
    let word = text
        .trim_start()
        .trim_start_matches('?')
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    word.rsplit('/').next().unwrap_or("").to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_found_past_prompts_and_libraries() {
        assert_eq!(command("?VFYCMN"), Some((COMMAND, "VFYCMN".into())));
        assert_eq!(command("qsys/wrkalr x(1)"), Some((COMMAND, "WRKALR".into())));
        assert_eq!(command("WRKACTJOB"), None);
        assert_eq!(menu("query"), Some(MENU));
        assert_eq!(menu("MAIN"), None);
    }

    #[test]
    fn the_lists_are_sorted_and_unique() {
        for l in [MENUS, COMMANDS] {
            assert!(l.windows(2).all(|w| w[0] < w[1]), "{l:?}");
        }
    }
}
