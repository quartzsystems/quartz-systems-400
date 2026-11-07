// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's commands, each with a definition as
//! `DescribeCommand` gives it:
//!
//! - `CRTLIB LIB(name) TYPE(*PROD|*TEST) TEXT()`: listed values; makes
//!   a library (`CPC2102`), `CPF2111` when it exists;
//! - `DSPOBJD OBJ(lib/name) OBJTYPE(list)`: a qualified name and a
//!   list of listed values; `OBJ(QSYS/SECRET)` is `NotAuthorized`
//!   (`CPF9802`);
//! - `CRTPF FILE(lib/name) SIZE(records increment number)`: an element
//!   list, `SIZE` under Additional Parameters;
//! - `STRSEU`: allowed only interactively, so it ends with `CPD0031`;
//! - `DLYJOB DLY(seconds)`: waits, and a `Cancel` ends it (`CPF1907`);
//! - `DLTUSRPRF USRPRF()`: the profile may not use it (`CPF9802`);
//! - `CPYFRMSTMF FROMSTMF() TOMBR()`: `CPCA081` (00), or `CPFA096`
//!   (20) when the stream file's name has `long` in it (lines cut);
//! - `CHGAUT OBJ() USER() DTAAUT()`: `CPDA091` (10) for a `DTAAUT` it
//!   does not take;
//! - `CHGOWN OBJ() NEWOWN() RVKOLDAUT() SYMLNK() SUBTREE()` and
//!   `CHGPGP OBJ() NEWPGP() DTAAUT() OBJAUT() RVKOLDAUT() SYMLNK()
//!   SUBTREE()`: `ifscmd`'s commands over the test system's file system
//!   ([`super::files`]) as the signed-on profile, with their messages;
//!   `SUBTREE(*ALL)` goes on past what it may not change, a diagnostic
//!   each, then the escape `CPFA0AB` (`Error`);
//! - `CALL PGM(lib/name)`: `HLINV/ASSETS` runs under an `Own` debug
//!   session ([`super::debug`]); any other program returns at once;
//! - `CHGCURLIB CURLIB(name)`: the job's current library
//!   ([`super::System::curlib`]); `CPF2110` for a library `CRTLIB` did
//!   not make.
//!
//! A missing required parameter is `CPD0071` then `CPF0006`; a command
//! not here, `CPD0030`. Messages carry the severities QS/400's message
//! files give them ([`severity`]), so every level from 00 to 40 shows.

use qap_proto::ops::command::{CommandDefinition, ParmKind, ParmNode, Range, Role, Show};
use qap_proto::types::ObjectName;

/// One command.
pub struct Cmd {
    /// The name.
    pub name: &'static str,
    /// The prompt text.
    pub text: &'static str,
    /// Only interactively.
    pub interactive_only: bool,
    /// The profile may use it.
    pub authorized: bool,
    /// Its required keywords.
    pub required: &'static [&'static str],
}

/// Every command.
pub const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "CRTLIB",
        text: "Create Library",
        interactive_only: false,
        authorized: true,
        required: &["LIB"],
    },
    Cmd {
        name: "DSPOBJD",
        text: "Display Object Description",
        interactive_only: false,
        authorized: true,
        required: &["OBJ", "OBJTYPE"],
    },
    Cmd {
        name: "CRTPF",
        text: "Create Physical File",
        interactive_only: false,
        authorized: true,
        required: &["FILE"],
    },
    Cmd {
        name: "CALL",
        text: "Call Program",
        interactive_only: false,
        authorized: true,
        required: &["PGM"],
    },
    Cmd {
        name: "STRSEU",
        text: "Start Source Entry Utility",
        interactive_only: true,
        authorized: true,
        required: &[],
    },
    Cmd {
        name: "DLYJOB",
        text: "Delay Job",
        interactive_only: false,
        authorized: true,
        required: &["DLY"],
    },
    Cmd {
        name: "DLTUSRPRF",
        text: "Delete User Profile",
        interactive_only: false,
        authorized: false,
        required: &["USRPRF"],
    },
    Cmd {
        name: "CPYFRMSTMF",
        text: "Copy From Stream File",
        interactive_only: false,
        authorized: true,
        required: &["FROMSTMF", "TOMBR"],
    },
    Cmd {
        name: "CHGAUT",
        text: "Change Authority",
        interactive_only: false,
        authorized: true,
        required: &["OBJ", "USER"],
    },
    Cmd {
        name: "CHGOWN",
        text: "Change Owner",
        interactive_only: false,
        authorized: true,
        required: &["OBJ", "NEWOWN"],
    },
    Cmd {
        name: "CHGPGP",
        text: "Change Primary Group",
        interactive_only: false,
        authorized: true,
        required: &["OBJ", "NEWPGP"],
    },
    Cmd {
        name: "CHGCURLIB",
        text: "Change Current Library",
        interactive_only: false,
        authorized: true,
        required: &["CURLIB"],
    },
];

/// Message `id`'s severity as QS/400's message files give it (the
/// ones the test system sends); 40 for any other.
#[must_use]
pub fn severity(id: &str) -> u8 {
    match id {
        "CPC2102" | "CPCA081" | "TCP1A0A" | "TCP1A54" | "TCP1A58" => 0,
        "CPDA091" => 10,
        "CPFA096" => 20,
        "CPD0030" | "CPD0031" | "CPD0071" | "CPF0006" | "CPF1907" | "CPD9BAE" | "TCP1A53"
        | "TCP1A55" | "TCP1A59" => 30,
        _ if id.starts_with("SQL") => 30,
        _ => 40,
    }
}

/// The command `name`.
#[must_use]
pub fn find(name: &str) -> Option<&'static Cmd> {
    COMMANDS
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
}

/// A command string's name and its `KWD(value)` parameters.
#[must_use]
pub fn parse(text: &str) -> (String, Vec<(String, String)>) {
    let text = text.trim();
    let (name, rest) = text.split_once(' ').unwrap_or((text, ""));
    let mut parms = Vec::new();
    let mut rest = rest.trim();
    while let Some(open) = rest.find('(') {
        let kwd = rest[..open].trim().to_ascii_uppercase();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        parms.push((kwd, rest[open + 1..open + close].trim().to_string()));
        rest = rest[open + close + 1..].trim();
    }
    (name.to_ascii_uppercase(), parms)
}

/// A node with the usual values.
fn node(
    index: u16,
    parent: Option<u16>,
    role: Role,
    keyword: &str,
    prompt: &str,
    kind: ParmKind,
    length: u32,
) -> ParmNode {
    ParmNode {
        index,
        parent,
        role,
        keyword: keyword.into(),
        prompt: prompt.into(),
        kind,
        length,
        min_values: 0,
        max_values: 1,
        ..ParmNode::default()
    }
}

/// The qualifier pair of a qualified name at `index`, owned by `owner`.
fn qualified(out: &mut Vec<ParmNode>, owner: u16) {
    let at = u16::try_from(out.len()).unwrap_or(u16::MAX);
    let mut name = node(
        at,
        Some(owner),
        Role::Qualifier,
        "",
        "Name",
        ParmKind::Name,
        10,
    );
    name.required = true;
    name.min_values = 1;
    out.push(name);
    let mut lib = node(
        at + 1,
        Some(owner),
        Role::Qualifier,
        "",
        "Library",
        ParmKind::Name,
        10,
    );
    lib.default = Some("*LIBL".into());
    lib.special_values = vec!["*LIBL".into(), "*CURLIB".into()];
    out.push(lib);
}

/// A `*NO`/`*YES` parameter at `index` defaulting to `dft`.
fn yes_no(index: u16, keyword: &str, prompt: &str, dft: &str) -> ParmNode {
    let mut p = node(
        index,
        None,
        Role::Parameter,
        keyword,
        prompt,
        ParmKind::Integer,
        2,
    );
    p.default = Some(dft.into());
    p.special_values = vec!["*NO".into(), "*YES".into()];
    p
}

/// `SUBTREE(*NONE|*ALL)` at `index`.
fn subtree(index: u16) -> ParmNode {
    let mut p = node(
        index,
        None,
        Role::Parameter,
        "SUBTREE",
        "Directory subtree",
        ParmKind::Character,
        10,
    );
    p.default = Some("*NONE".into());
    p.values = vec!["*NONE".into(), "*ALL".into()];
    p
}

/// `CHGOWN`'s or `CHGPGP`'s nodes, as the shipped definitions give them.
fn owner_or_group(name: &str, out: &mut Vec<ParmNode>) {
    let mut obj = node(
        0,
        None,
        Role::Parameter,
        "OBJ",
        "Object",
        ParmKind::Path,
        5000,
    );
    obj.required = true;
    obj.min_values = 1;
    out.push(obj);
    if name == "CHGOWN" {
        let mut own = node(
            1,
            None,
            Role::Parameter,
            "NEWOWN",
            "New owner",
            ParmKind::Name,
            10,
        );
        own.required = true;
        own.min_values = 1;
        out.push(own);
        out.push(yes_no(
            2,
            "RVKOLDAUT",
            "Revoke current owner authority",
            "*YES",
        ));
        out.push(yes_no(3, "SYMLNK", "Symbolic link", "*NO"));
        out.push(subtree(4));
        return;
    }
    let mut group = node(
        1,
        None,
        Role::Parameter,
        "NEWPGP",
        "New primary group",
        ParmKind::Name,
        10,
    );
    group.required = true;
    group.min_values = 1;
    group.special_values = vec!["*NONE".into()];
    out.push(group);
    let mut data = node(
        2,
        None,
        Role::Parameter,
        "DTAAUT",
        "New data authority",
        ParmKind::Integer,
        2,
    );
    data.default = Some("*OLDPGP".into());
    data.special_values = [
        "*OLDPGP", "*PRIVATE", "*RWX", "*RX", "*RW", "*WX", "*R", "*W", "*X", "*EXCLUDE", "*NONE",
    ]
    .map(String::from)
    .to_vec();
    out.push(data);
    let mut objaut = node(
        3,
        None,
        Role::Parameter,
        "OBJAUT",
        "New object authority",
        ParmKind::Integer,
        2,
    );
    objaut.default = Some("*NONE".into());
    objaut.max_values = 4;
    objaut.special_values = [
        "*NONE",
        "*ALL",
        "*OBJEXIST",
        "*OBJMGT",
        "*OBJALTER",
        "*OBJREF",
    ]
    .map(String::from)
    .to_vec();
    out.push(objaut);
    out.push(yes_no(4, "RVKOLDAUT", "Revoke old authority", "*YES"));
    out.push(yes_no(5, "SYMLNK", "Symbolic link", "*NO"));
    out.push(subtree(6));
}

/// `name`'s definition.
#[must_use]
pub fn definition(c: &Cmd) -> CommandDefinition {
    let mut n: Vec<ParmNode> = Vec::new();
    let required = |mut p: ParmNode| {
        p.required = true;
        p.min_values = 1;
        p
    };
    match c.name {
        "CRTLIB" => {
            n.push(required(node(
                0,
                None,
                Role::Parameter,
                "LIB",
                "Library",
                ParmKind::Name,
                10,
            )));
            let mut ty = node(
                1,
                None,
                Role::Parameter,
                "TYPE",
                "Library type",
                ParmKind::Character,
                5,
            );
            ty.default = Some("*PROD".into());
            ty.values = vec!["*PROD".into(), "*TEST".into()];
            n.push(ty);
            let mut text = node(
                2,
                None,
                Role::Parameter,
                "TEXT",
                "Text 'description'",
                ParmKind::Character,
                50,
            );
            text.default = Some("*BLANK".into());
            text.special_values = vec!["*BLANK".into()];
            n.push(text);
        }
        "DSPOBJD" => {
            n.push(required(node(
                0,
                None,
                Role::Parameter,
                "OBJ",
                "Object",
                ParmKind::Name,
                0,
            )));
            qualified(&mut n, 0);
            let mut ty = required(node(
                3,
                None,
                Role::Parameter,
                "OBJTYPE",
                "Object type",
                ParmKind::Character,
                10,
            ));
            ty.max_values = 10;
            ty.values = vec!["*FILE".into(), "*PGM".into(), "*LIB".into(), "*CMD".into()];
            ty.special_values = vec!["*ALL".into()];
            n.push(ty);
        }
        "CRTPF" => {
            n.push(required(node(
                0,
                None,
                Role::Parameter,
                "FILE",
                "File",
                ParmKind::Name,
                0,
            )));
            qualified(&mut n, 0);
            let mut size = node(
                3,
                None,
                Role::Parameter,
                "SIZE",
                "Member size",
                ParmKind::Character,
                0,
            );
            size.show = Show::Additional;
            size.special_values = vec!["*NOMAX".into()];
            n.push(size);
            for (i, (prompt, dft)) in [
                ("Initial number of records", "10000"),
                ("Increment number of records", "1000"),
                ("Maximum increments", "3"),
            ]
            .into_iter()
            .enumerate()
            {
                let idx = 4 + u16::try_from(i).unwrap_or(0);
                let mut e = node(
                    idx,
                    Some(3),
                    Role::Element,
                    "",
                    prompt,
                    ParmKind::Integer,
                    10,
                );
                e.default = Some(dft.into());
                e.range = Some(Range {
                    low: "1".into(),
                    high: "2147483646".into(),
                });
                e.show = Show::Additional;
                n.push(e);
            }
        }
        "STRSEU" => {
            n.push(node(
                0,
                None,
                Role::Parameter,
                "SRCFILE",
                "Source file",
                ParmKind::Name,
                0,
            ));
            qualified(&mut n, 0);
        }
        "DLYJOB" => {
            let mut d = required(node(
                0,
                None,
                Role::Parameter,
                "DLY",
                "Delay time, in seconds",
                ParmKind::Integer,
                10,
            ));
            d.range = Some(Range {
                low: "1".into(),
                high: "999999".into(),
            });
            n.push(d);
        }
        "CPYFRMSTMF" => {
            n.push(required(node(
                0,
                None,
                Role::Parameter,
                "FROMSTMF",
                "From stream file",
                ParmKind::Path,
                5000,
            )));
            n.push(required(node(
                1,
                None,
                Role::Parameter,
                "TOMBR",
                "To file member or save file",
                ParmKind::Path,
                5000,
            )));
        }
        "CHGAUT" => {
            n.push(required(node(
                0,
                None,
                Role::Parameter,
                "OBJ",
                "Object",
                ParmKind::Path,
                5000,
            )));
            n.push(required(node(
                1,
                None,
                Role::Parameter,
                "USER",
                "User",
                ParmKind::Name,
                10,
            )));
            let mut d = node(
                2,
                None,
                Role::Parameter,
                "DTAAUT",
                "New data authorities",
                ParmKind::Character,
                10,
            );
            d.default = Some("*SAME".into());
            d.special_values = vec![
                "*SAME".into(),
                "*RWX".into(),
                "*RX".into(),
                "*R".into(),
                "*EXCLUDE".into(),
            ];
            n.push(d);
        }
        "CHGOWN" | "CHGPGP" => owner_or_group(c.name, &mut n),
        "CHGCURLIB" => {
            let mut c = required(node(
                0,
                None,
                Role::Parameter,
                "CURLIB",
                "Current library",
                ParmKind::Name,
                10,
            ));
            c.special_values = vec!["*CRTDFT".into()];
            n.push(c);
        }
        _ => n.push(required(node(
            0,
            None,
            Role::Parameter,
            "USRPRF",
            "User profile",
            ParmKind::Name,
            10,
        ))),
    }
    CommandDefinition {
        name: ObjectName {
            library: "QSYS".into(),
            name: c.name.into(),
        },
        text: c.text.into(),
        help: String::new(),
        nodes: n,
    }
}
