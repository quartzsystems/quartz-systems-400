// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keyword groups the Select File, Record and Field Keywords
//! displays offer, with the row each takes, and which group a DDS
//! keyword falls in. Display attributes and colors are their own
//! displays; every other group is edited as keyword text.

use dds::lex::Keyword;
pub use dds::lex::keyword::CONSTANT;

use crate::source::{Element, Kind, Usage};

/// Which level a group belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// File level (F14).
    File,
    /// Record level (option 8).
    Record,
    /// A field or constant (`*` on the image, option 1 on Work with
    /// Fields).
    Field,
}

/// One group on a Select Keywords display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    /// Its name in the program.
    pub id: &'static str,
    /// The row it is on.
    pub row: usize,
    /// The label.
    pub label: &'static str,
    /// For which fields (field level), shown on the right.
    pub for_type: &'static str,
    /// The keywords in it; `*` ends a prefix, `##` is two digits
    /// (`CA##`).
    pub names: &'static [&'static str],
}

const KEYS: &[&str] = &[
    "CA##", "CF##", "ROLLUP", "ROLLDOWN", "PAGEUP", "PAGEDOWN", "CLEAR", "HOME", "HELP", "PRINT",
    "VLDCMDKEY", "SETOF", "SETOFF", "INDTXT", "INDARA",
];

/// The file-level groups.
pub const FILE: &[Group] = &[
    Group { id: "GENERAL", row: 8, label: "General keywords", for_type: "", names: &[] },
    Group { id: "INDICATOR", row: 9, label: "Indicator keywords", for_type: "", names: KEYS },
    Group { id: "PRINT", row: 10, label: "Print keywords", for_type: "", names: &["PRINT"] },
    Group { id: "HELP", row: 11, label: "Help keywords", for_type: "", names: &["HLP*", "ALTHELP"] },
    Group { id: "DSPSIZ", row: 12, label: "Display sizes", for_type: "", names: &["DSPSIZ"] },
    Group {
        id: "ALTERNATE",
        row: 13,
        label: "Alternate keywords",
        for_type: "",
        names: &["ALTPAGEUP", "ALTPAGEDWN", "ALTNAME"],
    },
    Group { id: "DBCS", row: 14, label: "DBCS conversion", for_type: "", names: &["IGCCNV"] },
    Group { id: "WINDOW", row: 15, label: "Window Borders", for_type: "", names: &["WDWBORDER"] },
    Group {
        id: "MENUBAR",
        row: 16,
        label: "Menu-bar keywords",
        for_type: "",
        names: &["MNUBARSW", "MNUCNL", "MNUBARSEP"],
    },
];

/// The record-level groups.
pub const RECORD: &[Group] = &[
    Group { id: "GENERAL", row: 8, label: "General keywords", for_type: "", names: &[] },
    Group { id: "INDICATOR", row: 9, label: "Indicator keywords", for_type: "", names: KEYS },
    Group { id: "APPHELP", row: 10, label: "Application help", for_type: "", names: &["HLPRTN", "HLPCMDKEY"] },
    Group { id: "HELP", row: 11, label: "Help keywords", for_type: "", names: &["HLP*"] },
    Group {
        id: "OUTPUT",
        row: 12,
        label: "Output keywords",
        for_type: "",
        names: &["ALARM", "BLINK", "LOCK", "CSRLOC", "SLNO", "CLRL", "FRCDTA", "DSPMOD"],
    },
    Group {
        id: "INPUT",
        row: 13,
        label: "Input keywords",
        for_type: "",
        names: &["CHANGE", "RTNCSRLOC", "RTNDTA", "INZRCD", "INVITE", "UNLOCK", "MDTOFF"],
    },
    Group {
        id: "OVERLAY",
        row: 14,
        label: "Overlay keywords",
        for_type: "",
        names: &[
            "OVERLAY", "PUTOVR", "PUTRETAIN", "OVRDTA", "OVRATR", "ERASE", "ERASEINP", "PROTECT",
            "KEEP", "ASSUME", "INZINP",
        ],
    },
    Group { id: "PRINT", row: 16, label: "Print keywords", for_type: "", names: &["PRINT"] },
    Group { id: "ALTNAME", row: 17, label: "ALTNAME keyword", for_type: "", names: &["ALTNAME"] },
    Group { id: "TEXT", row: 19, label: "TEXT keyword", for_type: "", names: &["TEXT"] },
];

/// The field-level groups.
pub const FIELD: &[Group] = &[
    Group { id: "DSPATR", row: 8, label: "Display attributes", for_type: "All except Hidden", names: &["DSPATR"] },
    Group { id: "COLOR", row: 9, label: "Colors", for_type: "All except Hidden", names: &["COLOR"] },
    Group {
        id: "INPUT",
        row: 10,
        label: "Input keywords",
        for_type: "Input or Both",
        names: &["AUTO", "BLANKS", "CHANGE", "CHGINPDFT", "DUP", "ENTFLDATR", "FLDCSRPRG"],
    },
    Group {
        id: "VALIDITY",
        row: 11,
        label: "Validity checking",
        for_type: "Input or Both",
        names: &["CHECK", "VALUES", "RANGE", "COMP", "CMP"],
    },
    Group { id: "GENERAL", row: 13, label: "General keywords", for_type: "All types", names: &[] },
    Group {
        id: "EDIT",
        row: 14,
        label: "Editing keywords",
        for_type: "Numeric Output or Both",
        names: &["EDTCDE", "EDTWRD", "EDTMSK"],
    },
    Group {
        id: "REF",
        row: 15,
        label: "Database reference",
        for_type: "Hidden, Input, Output, Both",
        names: &["REFFLD"],
    },
    Group {
        id: "ERRMSG",
        row: 16,
        label: "Error messages",
        for_type: "Input, Output, Both",
        names: &["ERRMSG", "ERRMSGID"],
    },
    Group { id: "MSGID", row: 17, label: "Message ID (MSGID)", for_type: "Output or Both", names: &["MSGID"] },
    Group { id: "TEXT", row: 20, label: "TEXT keyword", for_type: "", names: &["TEXT"] },
];

/// The groups of a level.
#[must_use]
pub fn groups(level: Level) -> &'static [Group] {
    match level {
        Level::File => FILE,
        Level::Record => RECORD,
        Level::Field => FIELD,
    }
}

/// `*` ends a prefix; `##` stands for two digits.
fn matches(pattern: &str, name: &str) -> bool {
    if let Some(p) = pattern.strip_suffix("##") {
        return name.len() == p.len() + 2
            && name.starts_with(p)
            && name[p.len()..].chars().all(|c| c.is_ascii_digit());
    }
    match pattern.strip_suffix('*') {
        Some(p) => name.starts_with(p) && name.len() > p.len(),
        None => pattern == name,
    }
}

/// The group a keyword belongs to at a level: the first that names
/// it, else General.
#[must_use]
pub fn group_of(level: Level, name: &str) -> &'static str {
    groups(level)
        .iter()
        .find(|g| g.names.iter().any(|p| matches(p, name)))
        .map_or("GENERAL", |g| g.id)
}

/// The groups that apply to an element: a constant has the attributes,
/// colors, general keywords and text; a field those its use allows.
#[must_use]
pub fn field_groups(e: &Element) -> Vec<Group> {
    let (usage, numeric) = match &e.kind {
        Kind::Constant { .. } => {
            return FIELD
                .iter()
                .filter(|g| matches!(g.id, "DSPATR" | "COLOR" | "GENERAL" | "TEXT"))
                .copied()
                .collect();
        }
        Kind::Field { usage, dec, ty, .. } => (
            *usage,
            dec.is_some() || matches!(ty, 'S' | 'Y' | 'P' | 'B' | 'F'),
        ),
    };
    let input = matches!(usage, Usage::Input | Usage::Both);
    let hidden = !usage.displayed();
    FIELD
        .iter()
        .filter(|g| match g.id {
            "DSPATR" | "COLOR" | "ERRMSG" => !hidden,
            "INPUT" | "VALIDITY" => input,
            "EDIT" => numeric && matches!(usage, Usage::Output | Usage::Both),
            "MSGID" => matches!(usage, Usage::Output | Usage::Both),
            _ => true,
        })
        .copied()
        .collect()
}

/// The keywords of a group out of a list, as text.
#[must_use]
pub fn texts(level: Level, group: &str, keywords: &[Keyword]) -> Vec<String> {
    keywords
        .iter()
        .filter(|k| k.name != dds::lex::keyword::CONSTANT && group_of(level, &k.name) == group)
        .map(ToString::to_string)
        .collect()
}

/// Replace a group's keywords in a list with ones parsed from text,
/// one or more per line, keeping the others where they were.
///
/// # Errors
///
/// A line the DDS keyword grammar does not take.
pub fn replace(
    level: Level,
    group: &str,
    keywords: &mut Vec<Keyword>,
    lines: &[String],
) -> Result<(), String> {
    let mut new = Vec::new();
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        new.extend(parse_keywords(l)?);
    }
    // Unchanged keywords keep their identity (and their source line).
    let old: Vec<Keyword> = keywords
        .iter()
        .filter(|k| k.name != dds::lex::keyword::CONSTANT && group_of(level, &k.name) == group)
        .cloned()
        .collect();
    let first = keywords
        .iter()
        .position(|k| k.name != dds::lex::keyword::CONSTANT && group_of(level, &k.name) == group);
    keywords.retain(|k| k.name == dds::lex::keyword::CONSTANT || group_of(level, &k.name) != group);
    let merged: Vec<Keyword> = new
        .into_iter()
        .map(|n| {
            old.iter()
                .find(|o| o.to_string() == n.to_string())
                .cloned()
                .unwrap_or(n)
        })
        .collect();
    let at = first.unwrap_or(keywords.len()).min(keywords.len());
    keywords.splice(at..at, merged);
    Ok(())
}

/// Keywords from text as the DDS keyword area holds them.
///
/// # Errors
///
/// Text the keyword grammar does not take.
pub fn parse_keywords(text: &str) -> Result<Vec<Keyword>, String> {
    let lines: Vec<String> = crate::source::write::function_lines(text)
        .iter()
        .map(crate::source::Line::text)
        .collect();
    let (source, _) = dds::Source::from_text(&lines.join("
"));
    let mut diags = dds::diag::Diagnostics::default();
    let specs = dds::lex::lex(&source, &mut diags);
    if let Some(d) = diags.as_slice().iter().find(|d| d.severity >= dds::Severity::Error) {
        return Err(d.text.clone());
    }
    Ok(specs
        .into_iter()
        .flat_map(|s| s.keywords)
        .map(|mut k| {
            k.line = usize::MAX;
            k
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_of_keywords() {
        assert_eq!(group_of(Level::Record, "CA03"), "INDICATOR");
        assert_eq!(group_of(Level::Record, "OVERLAY"), "OVERLAY");
        assert_eq!(group_of(Level::Record, "CAPTION"), "GENERAL");
        assert_eq!(group_of(Level::Field, "EDTCDE"), "EDIT");
        assert_eq!(group_of(Level::File, "DSPSIZ"), "DSPSIZ");
    }

    #[test]
    fn replace_a_group() {
        let mut kws = parse_keywords("CA03(03 'Exit') OVERLAY TEXT('A screen')").unwrap();
        replace(Level::Record, "INDICATOR", &mut kws, &["CF05(05)".into(), "CA03(03 'Exit')".into()])
            .unwrap();
        let t: Vec<String> = kws.iter().map(ToString::to_string).collect();
        assert_eq!(t, ["CF05(05)", "CA03(03 'Exit')", "OVERLAY", "TEXT('A screen')"]);
        assert!(replace(Level::Record, "GENERAL", &mut kws, &["X(".into()]).is_err());
    }
}
