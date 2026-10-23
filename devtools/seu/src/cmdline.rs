// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `SEU==>` command line: `FIND`, `CHANGE`, `TOP`, `BOTTOM`,
//! `SAVE`, `FILE`, `CANCEL`, `SET`, `EXIT`, a sequence number to
//! position to, `+n`/`-n` to roll. Each command has its short form
//! (`F`, `C`, `T`, `B`, `CAN`).

use crate::find::{Direction, Scope};

/// One `SET` option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetOpt {
    /// `SET CAPS ON|OFF`: upper-case input.
    Caps(bool),
    /// `SET TABS ON|OFF`.
    Tabs(bool),
    /// `SET MATCH ON|OFF`: the kind of match of find and change.
    Match(bool),
    /// `SET FULL ON|OFF`: full screen mode.
    Full(bool),
    /// `SET ROLL H|F|C|D|n`.
    Roll(String),
}

/// Where `SAVE`/`FILE` write: another member, file or library.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveTo {
    /// The library; empty for the session's.
    pub lib: String,
    /// The file; empty for the session's.
    pub file: String,
    /// The member; empty for the session's.
    pub member: String,
}

/// The search of `FIND` and `CHANGE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    /// The string; `*` repeats the last one.
    pub what: String,
    /// The direction.
    pub dir: Direction,
    /// Which records.
    pub scope: Scope,
    /// From column, 1-based.
    pub from: Option<usize>,
    /// To column.
    pub to: Option<usize>,
}

/// A command line command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Find a string.
    Find(Search),
    /// Change a string to another.
    Change(Search, String),
    /// The first records.
    Top,
    /// The last records.
    Bottom,
    /// Save and go on editing.
    Save(SaveTo),
    /// Save and end.
    File(SaveTo),
    /// End without saving.
    Cancel,
    /// Change a session setting.
    Set(SetOpt),
    /// The Exit display.
    Exit,
    /// That sequence number to the top.
    Position(u32),
    /// Roll forward (positive) or back (negative) lines.
    Roll(i64),
}

/// Why a command line is no command.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CmdError {
    /// Not a command of the line.
    #[error("command {0} is not known")]
    Unknown(String),
    /// A command missing its operands or with ones it does not take.
    #[error("operands of {0} are not valid")]
    Operands(String),
    /// A quoted string with no end.
    #[error("a quoted string has no closing quote")]
    Quote,
}

/// The words of a command line; quoted strings keep their blanks and
/// lose their quotes (a doubled quote is one quote).
fn words(line: &str) -> Result<Vec<(String, bool)>, CmdError> {
    let mut out = Vec::new();
    let mut it = line.chars().peekable();
    while let Some(&c) = it.peek() {
        if c == ' ' {
            it.next();
            continue;
        }
        if c == '\'' || c == '"' {
            it.next();
            let mut s = String::new();
            loop {
                match it.next() {
                    None => return Err(CmdError::Quote),
                    Some(q) if q == c => {
                        if it.peek() == Some(&c) {
                            it.next();
                            s.push(c);
                        } else {
                            break;
                        }
                    }
                    Some(ch) => s.push(ch),
                }
            }
            out.push((s, true));
            continue;
        }
        let mut s = String::new();
        while let Some(&ch) = it.peek() {
            if ch == ' ' {
                break;
            }
            s.push(ch);
            it.next();
        }
        out.push((s, false));
    }
    Ok(out)
}

/// The options after a search string: direction, scope, columns.
fn search_opts(verb: &str, what: String, rest: &[(String, bool)]) -> Result<Search, CmdError> {
    let mut s = Search {
        what,
        dir: Direction::Next,
        scope: Scope::All,
        from: None,
        to: None,
    };
    for (w, _) in rest {
        let u = w.to_ascii_uppercase();
        match u.as_str() {
            "NEXT" | "N" => s.dir = Direction::Next,
            "PREV" | "PREVIOUS" | "P" => s.dir = Direction::Previous,
            "FIRST" => s.dir = Direction::First,
            "LAST" => s.dir = Direction::Last,
            "ALL" => s.dir = Direction::All,
            "X" => s.scope = Scope::Excluded,
            "NX" => s.scope = Scope::NotExcluded,
            n if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
                let v: usize = n.parse().map_err(|_| CmdError::Operands(verb.into()))?;
                if s.from.is_none() {
                    s.from = Some(v);
                } else if s.to.is_none() {
                    s.to = Some(v);
                } else {
                    return Err(CmdError::Operands(verb.into()));
                }
            }
            _ => return Err(CmdError::Operands(verb.into())),
        }
    }
    Ok(s)
}

fn on_off(verb: &str, w: Option<&(String, bool)>) -> Result<bool, CmdError> {
    match w.map(|(s, _)| s.to_ascii_uppercase()).as_deref() {
        Some("ON") => Ok(true),
        Some("OFF") => Ok(false),
        _ => Err(CmdError::Operands(verb.into())),
    }
}

/// `[lib/]file [member]` after `SAVE` and `FILE`.
fn save_to(verb: &str, rest: &[(String, bool)]) -> Result<SaveTo, CmdError> {
    let mut t = SaveTo::default();
    match rest {
        [] => {}
        [(f, _)] | [(f, _), _] => {
            let f = f.to_ascii_uppercase();
            if let Some((l, n)) = f.split_once('/') {
                t.lib = l.to_string();
                t.file = n.to_string();
            } else {
                t.file = f;
            }
            if let [_, (m, _)] = rest {
                t.member = m.to_ascii_uppercase();
            }
        }
        _ => return Err(CmdError::Operands(verb.into())),
    }
    Ok(t)
}

/// Read a command line. `None` for a blank one.
pub fn parse(line: &str) -> Result<Option<Command>, CmdError> {
    let ws = words(line)?;
    let Some(((first, quoted), rest)) = ws.split_first() else {
        return Ok(None);
    };
    let verb = first.to_ascii_uppercase();
    if *quoted {
        return Err(CmdError::Unknown(first.clone()));
    }
    if let Some(n) = verb.strip_prefix('+').or_else(|| verb.strip_prefix('-')) {
        let v: i64 = if n.is_empty() {
            1
        } else {
            n.parse().map_err(|_| CmdError::Unknown(verb.clone()))?
        };
        return Ok(Some(Command::Roll(if verb.starts_with('-') { -v } else { v })));
    }
    if verb.chars().all(|c| c.is_ascii_digit() || c == '.') {
        let seq = crate::parse_seq(&verb).ok_or_else(|| CmdError::Unknown(verb.clone()))?;
        return Ok(Some(Command::Position(seq)));
    }
    let none = |c: Command| {
        if rest.is_empty() {
            Ok(Some(c))
        } else {
            Err(CmdError::Operands(verb.clone()))
        }
    };
    match verb.as_str() {
        "F" | "FIND" => {
            let Some(((what, _), opts)) = rest.split_first() else {
                return Err(CmdError::Operands(verb));
            };
            Ok(Some(Command::Find(search_opts(&verb, what.clone(), opts)?)))
        }
        "C" | "CHANGE" => {
            let [(old, _), (new, _), opts @ ..] = rest else {
                return Err(CmdError::Operands(verb));
            };
            Ok(Some(Command::Change(
                search_opts(&verb, old.clone(), opts)?,
                new.clone(),
            )))
        }
        "T" | "TOP" => none(Command::Top),
        "B" | "BOTTOM" | "BOT" => none(Command::Bottom),
        "CAN" | "CANCEL" => none(Command::Cancel),
        "EXIT" => none(Command::Exit),
        "SAVE" => Ok(Some(Command::Save(save_to(&verb, rest)?))),
        "FILE" => Ok(Some(Command::File(save_to(&verb, rest)?))),
        "SET" => {
            let Some(((opt, _), args)) = rest.split_first() else {
                return Err(CmdError::Operands(verb));
            };
            let o = opt.to_ascii_uppercase();
            let set = match o.as_str() {
                "CAPS" => SetOpt::Caps(on_off(&verb, args.first())?),
                "TABS" => SetOpt::Tabs(on_off(&verb, args.first())?),
                "MATCH" => SetOpt::Match(on_off(&verb, args.first())?),
                "FULL" => SetOpt::Full(on_off(&verb, args.first())?),
                "ROLL" => {
                    let v = args
                        .first()
                        .map(|(s, _)| s.to_ascii_uppercase())
                        .filter(|s| crate::defaults::Roll::parse(s).is_some())
                        .ok_or_else(|| CmdError::Operands(verb.clone()))?;
                    SetOpt::Roll(v)
                }
                _ => return Err(CmdError::Operands(verb)),
            };
            if args.len() > 1 {
                return Err(CmdError::Operands(verb));
            }
            Ok(Some(Command::Set(set)))
        }
        _ => Err(CmdError::Unknown(verb)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_forms() {
        let Some(Command::Find(s)) = parse("find IID").unwrap() else {
            panic!()
        };
        assert_eq!((s.what.as_str(), s.dir), ("IID", Direction::Next));
        let Some(Command::Find(s)) = parse("f 'two words' prev 10 20").unwrap() else {
            panic!()
        };
        assert_eq!(s.what, "two words");
        assert_eq!((s.dir, s.from, s.to), (Direction::Previous, Some(10), Some(20)));
        let Some(Command::Find(s)) = parse("F 'it''s' all nx").unwrap() else {
            panic!()
        };
        assert_eq!((s.what.as_str(), s.scope), ("it's", Scope::NotExcluded));
        assert!(parse("find").is_err());
        assert!(parse("find 'open").is_err());
    }

    #[test]
    fn change_forms() {
        let Some(Command::Change(s, new)) = parse("c old new all").unwrap() else {
            panic!()
        };
        assert_eq!((s.what.as_str(), new.as_str(), s.dir), ("old", "new", Direction::All));
        assert!(parse("change old").is_err());
    }

    #[test]
    fn the_rest() {
        assert_eq!(parse("top").unwrap(), Some(Command::Top));
        assert_eq!(parse("B").unwrap(), Some(Command::Bottom));
        assert_eq!(parse("can").unwrap(), Some(Command::Cancel));
        assert_eq!(parse("exit").unwrap(), Some(Command::Exit));
        assert_eq!(parse("  ").unwrap(), None);
        assert_eq!(parse("12.5").unwrap(), Some(Command::Position(1250)));
        assert_eq!(parse("+5").unwrap(), Some(Command::Roll(5)));
        assert_eq!(parse("-").unwrap(), Some(Command::Roll(-1)));
        assert_eq!(parse("set caps off").unwrap(), Some(Command::Set(SetOpt::Caps(false))));
        assert_eq!(parse("set roll 5").unwrap(), Some(Command::Set(SetOpt::Roll("5".into()))));
        assert!(parse("set roll x").is_err());
        assert!(parse("xyz").is_err());
        assert!(parse("top 3").is_err());
        let Some(Command::Save(t)) = parse("save mylib/qclsrc new").unwrap() else {
            panic!()
        };
        assert_eq!((t.lib.as_str(), t.file.as_str(), t.member.as_str()), ("MYLIB", "QCLSRC", "NEW"));
        assert_eq!(parse("file").unwrap(), Some(Command::File(SaveTo::default())));
    }
}
