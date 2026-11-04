// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Secondary language libraries (`QSYS29xx`): the language's text for
//! the system message files and the display files' constants, from a
//! source under `msgs/nls/`. A message the source does not translate
//! keeps the primary language's first-level text; none carries
//! second-level text.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{MsgfError, Result};
use crate::source::{MsgFile, builtin};

/// The sources by feature code.
pub const SOURCES: &[(u16, &str)] = &[(2931, include_str!("../msgs/nls/2931.toml"))];

#[derive(Deserialize)]
struct LangToml {
    feature: u16,
    ccsid: u16,
    library: String,
    #[serde(default)]
    marker: String,
    #[serde(default)]
    files: BTreeMap<String, String>,
    #[serde(default)]
    constants: BTreeMap<String, String>,
    #[serde(flatten)]
    texts: BTreeMap<String, BTreeMap<String, String>>,
}

/// One secondary language's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    /// The feature code, `2931`.
    pub feature: u16,
    /// The CCSID its text is in.
    pub ccsid: u16,
    /// The library's text description.
    pub library_text: String,
    /// What each file's text description ends with (`(scaffold...)`).
    pub marker: String,
    /// The files it carries, with their text descriptions.
    pub files: BTreeMap<String, String>,
    /// Display file constants: the primary's, the language's.
    pub constants: BTreeMap<String, String>,
    /// First-level texts by file and identifier.
    pub texts: BTreeMap<String, BTreeMap<String, String>>,
}

impl Language {
    /// Parse a language source.
    pub fn parse(text: &str) -> Result<Self> {
        let t: LangToml = toml::from_str(text).map_err(|e| MsgfError::Source(e.to_string()))?;
        Ok(Self {
            feature: t.feature,
            ccsid: t.ccsid,
            library_text: t.library,
            marker: t.marker,
            files: t.files,
            constants: t.constants,
            texts: t.texts,
        })
    }

    /// The language's library, `QSYS2931`.
    #[must_use]
    pub fn library(&self) -> String {
        format!("QSYS{}", self.feature)
    }

    /// Its message files: each system file it names, with the
    /// language's first-level texts and CCSID and no second-level text.
    #[must_use]
    pub fn message_files(&self) -> Vec<MsgFile> {
        builtin()
            .into_iter()
            .filter_map(|mut f| {
                let text = self.files.get(&f.name)?;
                let mine = self.texts.get(&f.name);
                f.text = format!("{text} {}", self.marker).trim().to_string();
                f.ccsid = self.ccsid;
                for m in &mut f.msgs {
                    if let Some(t) = mine.and_then(|x| x.get(&m.id)) {
                        m.text.clone_from(t);
                    }
                    m.help.clear();
                }
                Some(f)
            })
            .collect()
    }

    /// A display file's DDS with its constants in the language.
    #[must_use]
    pub fn translate_dds(&self, src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        for line in src.lines() {
            let mut l = line.to_string();
            for (from, to) in &self.constants {
                let q = format!("'{from}'");
                if l.contains(&q) {
                    l = l.replace(&q, &format!("'{to}'"));
                }
            }
            out.push_str(&l);
            out.push('\n');
        }
        out
    }

    /// Messages with the language's text, and all messages, of its files.
    #[must_use]
    pub fn coverage(&self) -> (usize, usize) {
        let files: Vec<MsgFile> = builtin().into_iter().filter(|f| self.files.contains_key(&f.name)).collect();
        let total = files.iter().map(|f| f.msgs.len()).sum();
        let done = files
            .iter()
            .map(|f| {
                let mine = self.texts.get(&f.name);
                f.msgs.iter().filter(|m| mine.is_some_and(|x| x.contains_key(&m.id))).count()
            })
            .sum();
        (done, total)
    }
}

/// The language of a feature code, when the system ships its text.
#[must_use]
pub fn language(feature: u16) -> Option<Language> {
    SOURCES
        .iter()
        .find(|(f, _)| *f == feature)
        .map(|(_, s)| Language::parse(s).expect("the language sources parse"))
}

/// The feature codes the system ships text for.
#[must_use]
pub fn features() -> Vec<u16> {
    SOURCES.iter().map(|(f, _)| *f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_scaffold() {
        let l = language(2931).unwrap();
        assert_eq!(l.library(), "QSYS2931");
        assert!(l.library_text.contains("SCAFFOLD"));
        let files = l.message_files();
        assert_eq!(files.len(), 2);
        let cpf = files.iter().find(|f| f.name == "QCPFMSG").unwrap();
        assert_eq!(cpf.ccsid, 284);
        assert!(cpf.text.ends_with("(SCAFFOLD)") && cpf.text.len() <= 50);
        assert_eq!(cpf.find("CPF9810").unwrap().text, "No se ha encontrado la biblioteca &1.");
        assert!(cpf.msgs.iter().all(|m| m.help.is_empty()));
        // Every message of the primary's file is there.
        let en = builtin().into_iter().find(|f| f.name == "QCPFMSG").unwrap();
        assert_eq!(en.msgs.len(), cpf.msgs.len());
        // Every translated identifier exists in its file.
        for (file, texts) in &l.texts {
            let f = files.iter().find(|x| &x.name == file).unwrap_or_else(|| panic!("{file}"));
            for id in texts.keys() {
                assert!(f.find(id).is_some(), "{file} {id}");
            }
        }
        let dds = l.translate_dds(SIGNON);
        assert!(dds.contains("'Inicio de sesión'") && dds.contains("'Usuario . ."), "{dds}");
        let (done, total) = l.coverage();
        assert!(done > 50 && done < total);
    }

    const SIGNON: &str = "     A                                  1 36'Sign On'\n     A                                  6 17'User  . . . . . . . . . . . . . .'\n";
}
