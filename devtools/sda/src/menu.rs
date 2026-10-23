// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Menu design (option 2): a menu is a display-file member of type
//! `MNUDDS` whose record named like the menu is its image, with a
//! command-line record `CMDTXT1`, and a member `<menu>QQ` of type
//! `MNUCMD` holding the command each option runs. Saving makes the
//! display file, the message file `<menu>QQ` (one message per option,
//! `USR0001` for option 1) and the `*MENU` of type `*DSPF`.

use std::collections::BTreeMap;

use dds::lex::{Keyword, Parm};

use crate::source::{Element, Kind, Member, Record, RecordType, Usage};

/// The command-line record a designed menu carries.
pub const CMDLINE_RECORD: &str = "CMDTXT1";

/// The commands member's name for a menu: the name with `QQ`, cut so it
/// stays a name.
#[must_use]
pub fn commands_member(menu: &str) -> String {
    let n: String = menu.trim().to_ascii_uppercase().chars().take(8).collect();
    format!("{n}QQ")
}

/// The message identifier an option's command is kept under.
#[must_use]
pub fn message_id(option: u8) -> String {
    format!("USR{option:04}")
}

fn constant(text: &str, row: u16, col: u16) -> Element {
    Element::new(
        Kind::Constant {
            text: text.to_string(),
            keyword: None,
        },
        Some((row, col)),
    )
}

fn kw(name: &str, parms: &[&str]) -> Keyword {
    Keyword {
        name: name.into(),
        parms: parms.iter().map(|p| Parm::Word((*p).into())).collect(),
        line: 0,
    }
}

/// A new menu member: the menu record with the usual layout and the
/// command-line record.
#[must_use]
pub fn template(menu: &str) -> Member {
    let name = menu.trim().to_ascii_uppercase();
    let mut m = Member {
        keywords: vec![
            kw("DSPSIZ", &["24", "80", "*DS3"]),
            kw("PRINT", &["*LIBL/QSYSPRT"]),
        ],
        ..Member::default()
    };
    let mut r = Record::new(&name, RecordType::Record);
    let mut title = constant(&name, 1, 2);
    title.colors.push("BLU".into());
    r.elements.push(title);
    r.elements.push(constant("Select one of the following:", 3, 2));
    for (i, row) in (5..=7).enumerate() {
        r.elements.push(constant(&format!("{}.", i + 1), row, 7));
    }
    m.records.push(r);
    let mut c = Record::new(CMDLINE_RECORD, RecordType::Record);
    c.keywords = vec![kw("OVERLAY", &[]), kw("CLRL", &["*NO"])];
    c.elements.push(constant("Selection or command", 19, 2));
    c.elements.push(constant("===>", 20, 2));
    c.elements.push(constant(
        "F3=Exit   F4=Prompt   F9=Retrieve   F12=Cancel",
        22,
        2,
    ));
    m.records.push(c);
    m
}

/// The option numbers the menu image shows: constants `n.` with `n`
/// from 1 to 99.
#[must_use]
pub fn options(record: &Record) -> Vec<u8> {
    let mut out: Vec<u8> = record
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Constant { text, keyword: None } => text
                .trim()
                .strip_suffix('.')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=99).contains(n)),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The commands member's lines: the option in columns 1-2, the command
/// from column 4.
#[must_use]
pub fn commands_source(commands: &BTreeMap<u8, String>) -> Vec<String> {
    commands
        .iter()
        .filter(|(_, c)| !c.trim().is_empty())
        .map(|(o, c)| format!("{o:02} {}", c.trim()))
        .collect()
}

/// The commands member read back.
#[must_use]
pub fn commands_parse(lines: &[String]) -> BTreeMap<u8, String> {
    lines
        .iter()
        .filter_map(|l| {
            let (n, c) = l.split_at(l.len().min(2));
            Some((n.trim().parse::<u8>().ok()?, c.trim().to_string()))
        })
        .filter(|(_, c)| !c.is_empty())
        .collect()
}

/// The CL that makes a designed menu's objects from its members: the
/// display file, the message file with a message per command, and the
/// menu.
#[must_use]
pub fn create_commands(
    menu: &str,
    srcfile: &str,
    srclib: &str,
    objlib: &str,
    commands: &BTreeMap<u8, String>,
) -> Vec<String> {
    let name = menu.trim().to_ascii_uppercase();
    let qq = commands_member(&name);
    let mut out = vec![
        format!("CRTDSPF FILE({objlib}/{name}) SRCFILE({srclib}/{srcfile}) SRCMBR({name})"),
        format!("CRTMSGF MSGF({objlib}/{qq})"),
    ];
    for (o, c) in commands.iter().filter(|(_, c)| !c.trim().is_empty()) {
        out.push(format!(
            "ADDMSGD MSGID({}) MSGF({objlib}/{qq}) MSG('{}')",
            message_id(*o),
            c.trim().replace('\'', "''")
        ));
    }
    out.push(format!(
        "CRTMNU MENU({objlib}/{name}) TYPE(*DSPF) DSPF({objlib}/{name}) MSGF({objlib}/{qq})"
    ));
    out
}

/// Whether a record holds a field the menu cannot have (menus show
/// only constants).
#[must_use]
pub fn has_fields(record: &Record) -> bool {
    record.elements.iter().any(|e| {
        matches!(
            e.kind,
            Kind::Field {
                usage: Usage::Input | Usage::Both | Usage::Output,
                ..
            }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Stamp, parse, write};

    #[test]
    fn a_new_menu_round_trips() {
        let m = template("MYMENU");
        let stamp = Stamp {
            date: "20261023".into(),
            time: "120000".into(),
            user: "QPGMR".into(),
            release: "V1R0M0".into(),
        };
        let lines = write(&m, &stamp);
        assert!(lines.iter().any(|l| l.ends_with("R MYMENU")));
        assert!(lines.iter().any(|l| l.ends_with("R CMDTXT1")));
        let back = parse(&lines);
        assert_eq!(options(&back.records[0]), [1, 2, 3]);
        assert_eq!(write(&back, &stamp), lines);
        // It compiles as a display file.
        let (src, _) = dds::Source::from_text(&lines.join("\n"));
        let c = dspf::compile_dspf(&src, &dds::NoRefs, &dds::CompileOpts::default());
        assert!(c.definition.is_some(), "{:#?}", c.diagnostics);
    }

    #[test]
    fn commands() {
        let mut c = BTreeMap::new();
        c.insert(1, "WRKSPLF".to_string());
        c.insert(12, "CALL PGM(X) PARM('a')".to_string());
        let src = commands_source(&c);
        assert_eq!(src, ["01 WRKSPLF", "12 CALL PGM(X) PARM('a')"]);
        assert_eq!(commands_parse(&src), c);
        let cl = create_commands("MYMENU", "QDDSSRC", "MYLIB", "MYLIB", &c);
        assert_eq!(cl.len(), 5);
        assert!(cl[3].contains("MSGID(USR0012)") && cl[3].contains("PARM(''a'')"));
        assert!(cl[4].starts_with("CRTMNU MENU(MYLIB/MYMENU) TYPE(*DSPF)"));
        assert_eq!(commands_member("LONGMENUNM"), "LONGMENUQQ");
    }
}
