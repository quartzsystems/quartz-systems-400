// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The flags of `EXPORTFS OPTIONS` (docs/NFS.md section 6.1):
//!
//! | Flag | Meaning |
//! |---|---|
//! | `-A` | every entry of `/etc/exports` (with `-U`: end every one's export) |
//! | `-I` | `DIR` with the options given here, `/etc/exports` not read |
//! | `-U` | end the export of `DIR` (or with `-A`, of every entry) |
//! | `-F` | with `-I`: `/etc/exports` is changed too (the entry put in, or with `-U` taken out) |
//! | `-O list` | the export options, in quotes when they hold blanks |
//! | `-E` | an entry that fails ends the command with an escape message |
//! | `-V` | a message for every entry exported or not |
//!
//! `*DFT` is no flag: `DIR` is exported with the options of its entry
//! in `/etc/exports`, or the defaults when it has none.

use super::error::{FlagError, OptError};
use super::model::ExportOptions;
use super::options;

/// The flags given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the flags are flags
pub struct Flags {
    /// `-A`.
    pub all: bool,
    /// `-I`.
    pub ignore: bool,
    /// `-U`.
    pub unexport: bool,
    /// `-F`.
    pub file: bool,
    /// `-O`: the option list, as given.
    pub options: Option<String>,
    /// `-E`.
    pub escape: bool,
    /// `-V`.
    pub verbose: bool,
}

/// What the flags and `DIR` ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `-A`: export every entry of `/etc/exports`.
    ExportAll,
    /// `-A -U`: end the export of every entry of `/etc/exports`.
    UnexportAll,
    /// Export one directory.
    Export {
        /// The directory.
        dir: String,
        /// `-I`: `/etc/exports` is not read.
        ignore_file: bool,
        /// `-O`: the options, which win over the file's.
        options: Option<ExportOptions>,
        /// `-F`: the entry is written to `/etc/exports`.
        update_file: bool,
    },
    /// `-U`: end the export of one directory.
    Unexport {
        /// The directory.
        dir: String,
        /// `-F`: the entry is taken out of `/etc/exports`.
        update_file: bool,
    },
}

/// Parse `OPTIONS`; `*DFT` and blanks are no flag.
pub fn parse(text: &str) -> Result<Flags, OptError> {
    let mut f = Flags::default();
    if text.trim().eq_ignore_ascii_case("*DFT") {
        return Ok(f);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let pos = i + 1;
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let word: String = chars[start..i]
            .iter()
            .collect::<String>()
            .to_ascii_uppercase();
        let dup = |set: bool| {
            if set {
                Err(OptError::Duplicate { name: word.clone() })
            } else {
                Ok(true)
            }
        };
        match word.as_str() {
            "-A" => f.all = dup(f.all)?,
            "-I" => f.ignore = dup(f.ignore)?,
            "-U" => f.unexport = dup(f.unexport)?,
            "-F" => f.file = dup(f.file)?,
            "-E" => f.escape = dup(f.escape)?,
            "-V" => f.verbose = dup(f.verbose)?,
            "-O" => {
                if f.options.is_some() {
                    return Err(OptError::Duplicate { name: word });
                }
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(OptError::Unknown { pos });
                }
                let q = chars[i];
                let list = if q == '\'' || q == '"' {
                    let from = i + 1;
                    let close = chars[from..]
                        .iter()
                        .position(|c| *c == q)
                        .ok_or(OptError::Unknown { pos: i + 1 })?;
                    i = from + close + 1;
                    chars[from..from + close].iter().collect()
                } else {
                    let from = i;
                    while i < chars.len() && !chars[i].is_whitespace() {
                        i += 1;
                    }
                    chars[from..i].iter().collect()
                };
                f.options = Some(list);
            }
            _ => return Err(OptError::Unknown { pos }),
        }
    }
    Ok(f)
}

impl Flags {
    /// What the flags ask for with `DIR` (`None` when not given): the
    /// combinations IBM's command takes, or its message.
    pub fn request(&self, dir: Option<&str>) -> Result<Request, FlagError> {
        let dir = dir.map(str::trim).filter(|d| !d.is_empty());
        if self.all {
            if self.ignore || self.options.is_some() || self.file {
                return Err(FlagError::Combination);
            }
            if dir.is_some() {
                return Err(FlagError::DirWithAll);
            }
            return Ok(if self.unexport {
                Request::UnexportAll
            } else {
                Request::ExportAll
            });
        }
        if self.file && !self.ignore {
            return Err(FlagError::Combination);
        }
        if self.unexport {
            if self.options.is_some() {
                return Err(FlagError::Combination);
            }
            let dir = dir.ok_or(FlagError::UnexportWhat)?;
            return Ok(Request::Unexport {
                dir: dir.to_string(),
                update_file: self.file,
            });
        }
        let dir = dir.ok_or(FlagError::DirNeeded)?;
        let options = self.options.as_deref().map(options::parse).transpose()?;
        Ok(Request::Export {
            dir: dir.to_string(),
            ignore_file: self.ignore,
            options,
            update_file: self.file,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(opts: &str, dir: Option<&str>) -> Result<Request, String> {
        parse(opts)
            .map_err(|e| e.msgid().to_string())?
            .request(dir)
            .map_err(|e| e.msgid().to_string())
    }

    #[test]
    fn flags() {
        let f = parse("-i -O 'ro, anon=0' -v -E").unwrap();
        assert!(f.ignore && f.verbose && f.escape);
        assert_eq!(f.options.as_deref(), Some("ro, anon=0"));
        assert_eq!(
            parse("-I -O ro,anon=0").unwrap().options.as_deref(),
            Some("ro,anon=0")
        );
        assert_eq!(parse("*DFT").unwrap(), Flags::default());
        assert_eq!(parse("-I -X"), Err(OptError::Unknown { pos: 4 }));
        assert_eq!(
            parse("-I -I"),
            Err(OptError::Duplicate { name: "-I".into() })
        );
        assert_eq!(parse("-O"), Err(OptError::Unknown { pos: 1 }));
        assert_eq!(parse("-O 'ro"), Err(OptError::Unknown { pos: 4 }));
    }

    #[test]
    fn combinations() {
        assert_eq!(req("-A", None), Ok(Request::ExportAll));
        assert_eq!(req("-A -U", None), Ok(Request::UnexportAll));
        assert_eq!(req("-A", Some("/x")), Err("CPFA1B3".into()));
        assert_eq!(req("-A -I", None), Err("CPFA1B2".into()));
        assert_eq!(req("-A -O ro", None), Err("CPFA1B2".into()));
        assert_eq!(req("-U -O ro", Some("/x")), Err("CPFA1B2".into()));
        assert_eq!(req("-F", Some("/x")), Err("CPFA1B2".into()));
        assert_eq!(req("-U", None), Err("CPFA1C4".into()));
        assert_eq!(req("-I", None), Err("CPFA1C3".into()));
        assert_eq!(req("-O ro", None), Err("CPFA1C3".into()));
        assert_eq!(req("*DFT", Some(" ")), Err("CPFA1C3".into()));
        assert_eq!(req("-O bogus", Some("/x")), Err("CPFA1B0".into()));
        assert_eq!(
            req("-U -I -F", Some("/x")),
            Ok(Request::Unexport {
                dir: "/x".into(),
                update_file: true
            })
        );
        let Ok(Request::Export {
            options: Some(o),
            ignore_file: true,
            update_file: true,
            ..
        }) = req("-I -F -O ro", Some("/x"))
        else {
            panic!("an export")
        };
        assert!(o.ro);
        assert_eq!(
            req("*DFT", Some("/x")),
            Ok(Request::Export {
                dir: "/x".into(),
                ignore_file: false,
                options: None,
                update_file: false
            })
        );
    }
}
