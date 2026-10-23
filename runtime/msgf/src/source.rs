// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message file sources: `msgs/*.toml` at build time, parsed into
//! descriptions. The system files are embedded here.

use serde::Deserialize;

use crate::desc::{MsgDesc, VarType};
use crate::error::{MsgfError, Result};

/// The source of `QCPFMSG`.
pub const QCPFMSG: &str = include_str!("../msgs/qcpfmsg.toml");
/// The source of `QCEEMSG`.
pub const QCEEMSG: &str = include_str!("../msgs/qceemsg.toml");
/// The source of `QSQLMSG`.
pub const QSQLMSG: &str = include_str!("../msgs/qsqlmsg.toml");
/// The source of `QRPGLEMSG`, the ILE RPG compiler's messages.
pub const QRPGLEMSG: &str = include_str!("../msgs/qrpglemsg.toml");
/// The source of `QRNXMSG`, the ILE RPG runtime's messages.
pub const QRNXMSG: &str = include_str!("../msgs/qrnxmsg.toml");

/// A message file: its name, text and descriptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgFile {
    /// Object name.
    pub name: String,
    /// Text description.
    pub text: String,
    /// Descriptions, in identifier order.
    pub msgs: Vec<MsgDesc>,
}

impl MsgFile {
    /// The description of an identifier.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&MsgDesc> {
        self.msgs
            .binary_search_by(|m| m.id.as_str().cmp(id))
            .ok()
            .map(|i| &self.msgs[i])
    }

    /// Add a description; an existing identifier is `CPF2499`.
    pub fn add(&mut self, d: MsgDesc) -> Result<()> {
        if !MsgDesc::valid_id(&d.id) {
            return Err(MsgfError::Id(d.id));
        }
        match self.msgs.binary_search_by(|m| m.id.cmp(&d.id)) {
            Ok(_) => Err(MsgfError::Exists(d.id)),
            Err(i) => {
                self.msgs.insert(i, d);
                Ok(())
            }
        }
    }

    /// Replace a description; a missing identifier is `CPF2419`.
    pub fn change(&mut self, d: MsgDesc) -> Result<()> {
        match self.msgs.binary_search_by(|m| m.id.cmp(&d.id)) {
            Ok(i) => {
                self.msgs[i] = d;
                Ok(())
            }
            Err(_) => Err(MsgfError::NotFound(d.id)),
        }
    }

    /// Remove a description.
    pub fn remove(&mut self, id: &str) -> Result<MsgDesc> {
        match self.msgs.binary_search_by(|m| m.id.as_str().cmp(id)) {
            Ok(i) => Ok(self.msgs.remove(i)),
            Err(_) => Err(MsgfError::NotFound(id.into())),
        }
    }

    /// Merge another file's descriptions in (`MRGMSGF`): new ones are
    /// added, existing ones replaced when `replace`; returns how many
    /// were added or replaced.
    pub fn merge(&mut self, from: &MsgFile, replace: bool) -> usize {
        let mut n = 0;
        for d in &from.msgs {
            match self.msgs.binary_search_by(|m| m.id.cmp(&d.id)) {
                Ok(i) if replace => {
                    self.msgs[i] = d.clone();
                    n += 1;
                }
                Ok(_) => {}
                Err(i) => {
                    self.msgs.insert(i, d.clone());
                    n += 1;
                }
            }
        }
        n
    }
}

#[derive(Deserialize)]
struct FileToml {
    name: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    msg: Vec<MsgToml>,
}

#[derive(Deserialize)]
struct ReplyToml {
    default: Option<String>,
    #[serde(default)]
    valid: Vec<String>,
}

#[derive(Deserialize)]
struct MsgToml {
    id: String,
    #[serde(default)]
    sev: u16,
    text: String,
    #[serde(default)]
    help: String,
    #[serde(default)]
    vars: Vec<String>,
    reply: Option<ReplyToml>,
    #[serde(default)]
    dump: Vec<u16>,
}

/// Parse a message file source.
pub fn parse(text: &str) -> Result<MsgFile> {
    let f: FileToml = toml::from_str(text).map_err(|e| MsgfError::Source(e.to_string()))?;
    let mut file = MsgFile {
        name: f.name,
        text: f.text,
        msgs: Vec::new(),
    };
    for m in f.msg {
        let vars = m
            .vars
            .iter()
            .map(|v| VarType::parse(v))
            .collect::<Result<Vec<_>>>()?;
        if m.severity_ok() {
            file.add(MsgDesc {
                id: m.id,
                severity: m.sev,
                text: m.text,
                help: m.help,
                vars,
                dft_reply: m.reply.as_ref().and_then(|r| r.default.clone()),
                valid_replies: m.reply.map(|r| r.valid).unwrap_or_default(),
                dump: m.dump,
            })?;
        } else {
            return Err(MsgfError::Source(format!(
                "{}: severity {} is not 0 to 99",
                m.id, m.sev
            )));
        }
    }
    Ok(file)
}

impl MsgToml {
    fn severity_ok(&self) -> bool {
        self.sev <= 99
    }
}

/// The system message files, parsed from their sources.
#[must_use]
pub fn builtin() -> Vec<MsgFile> {
    [QCPFMSG, QCEEMSG, QSQLMSG, QRPGLEMSG, QRNXMSG]
        .iter()
        .map(|s| parse(s).expect("the built-in sources parse"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_sources_parse_and_look_up() {
        let files = builtin();
        assert_eq!(files.len(), 5);
        let cpf = &files[0];
        assert_eq!(cpf.name, "QCPFMSG");
        let m = cpf.find("MCH1211").unwrap();
        assert_eq!(m.severity, 40);
        assert!(m.text.contains("zero"));
        let cee = files[1].find("CEE9901").unwrap();
        assert_eq!(cee.dft_reply.as_deref(), Some("C"));
        assert!(cee.reply_ok("g") && !cee.reply_ok("X"));
        assert_eq!(cee.vars.len(), 2);
        assert!(files[2].find("SQL0204").is_some());
        for f in &files {
            for w in f.msgs.windows(2) {
                assert!(w[0].id < w[1].id, "{} sorted", f.name);
            }
        }
    }

    #[test]
    fn edits_keep_order() {
        let mut f = parse("name = \"X\"\n[[msg]]\nid = \"XXX0002\"\ntext = \"b\"\n").unwrap();
        f.add(MsgDesc {
            id: "XXX0001".into(),
            severity: 0,
            text: "a".into(),
            help: String::new(),
            vars: vec![],
            dft_reply: None,
            valid_replies: vec![],
            dump: vec![],
        })
        .unwrap();
        assert_eq!(f.msgs[0].id, "XXX0001");
        assert!(f.add(f.msgs[0].clone()).is_err());
        assert!(f.remove("XXX0009").is_err());
        let other = parse("name = \"Y\"\n[[msg]]\nid = \"XXX0002\"\ntext = \"changed\"\n[[msg]]\nid = \"XXX0003\"\ntext = \"c\"\n").unwrap();
        assert_eq!(f.merge(&other, false), 1);
        assert_eq!(f.find("XXX0002").unwrap().text, "b");
        assert_eq!(f.merge(&other, true), 2);
        assert_eq!(f.find("XXX0002").unwrap().text, "changed");
    }
}
