// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debug command line: each command and its shortest abbreviation
//! (as IBM's source debugger accepts them), parsed into a [`Command`].
//! `EQUATE` names are expanded by [`Equates::expand`] before parsing.

use std::collections::BTreeMap;

use crate::DbgError;

/// A scroll amount: lines or columns, or as far as it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amount {
    /// The display's own amount (a page, or the shift).
    Default,
    /// This many.
    By(usize),
    /// `MAX`.
    Max,
}

/// Where a breakpoint goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// The cursor's line (no operand).
    Cursor,
    /// A line of the shown module.
    Line(usize),
    /// A line in a procedure: `PROC/line`.
    ProcLine(String, usize),
}

/// What `CLEAR` removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClearWhat {
    /// One breakpoint.
    At(Place),
    /// Every breakpoint of the program (`CLEAR PGM`).
    Pgm,
    /// A watch by number.
    Watch(usize),
    /// Every watch (`CLEAR WATCH ALL`).
    WatchAll,
}

/// What `DISPLAY` shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayWhat {
    /// The breakpoints as a message.
    Breakpoints,
    /// The equates as a message.
    Equates,
    /// The watches.
    Watch,
    /// Another module's source.
    Module(String),
}

/// A `SET` option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetOption {
    /// `SET UPDPROD YES|NO`.
    UpdProd(bool),
    /// `SET CASE MATCH|NOMATCH`: whether `FIND` matches case.
    Case(bool),
    /// `SET OPMSRC YES|NO`.
    OpmSrc(bool),
}

/// `EVAL`'s format suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// As the variable's type shows it.
    Natural,
    /// `: X [n]` hexadecimal.
    Hex,
    /// `: C [n]` characters.
    Char,
}

/// One debug command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `BREAK [place] [WHEN expr]`.
    Break {
        /// Where.
        at: Place,
        /// A condition.
        when: Option<String>,
    },
    /// `TBREAK [place] [WHEN expr]`: a breakpoint for the current
    /// thread only.
    TBreak {
        /// Where.
        at: Place,
        /// A condition.
        when: Option<String>,
    },
    /// `CLEAR ...`.
    Clear(ClearWhat),
    /// `DISPLAY ...`.
    Display(DisplayWhat),
    /// `EVAL expr [: X|C [n]]`, or an assignment `EVAL var = value`.
    Eval {
        /// The expression (or the target of an assignment).
        expr: String,
        /// The value assigned.
        assign: Option<String>,
        /// The format.
        format: Format,
        /// The length for `: X n` and `: C n`.
        len: Option<usize>,
    },
    /// `WATCH expr [: n]`.
    Watch {
        /// The variable.
        expr: String,
        /// Bytes watched.
        len: Option<usize>,
    },
    /// `STEP [n] [INTO|OVER]`.
    Step {
        /// How many statements.
        count: usize,
        /// Into called programs.
        into: bool,
    },
    /// `ATTR var`.
    Attr(String),
    /// `QUAL line`: the scope `EVAL` uses.
    Qual(usize),
    /// `FIND 'string'|line [NEXT|PREVIOUS]`.
    Find {
        /// The string, empty for the last one.
        what: String,
        /// Backwards.
        back: bool,
    },
    /// `UP [n|MAX]`.
    Up(Amount),
    /// `DOWN [n|MAX]`.
    Down(Amount),
    /// `LEFT [n|MAX]`.
    Left(Amount),
    /// `RIGHT [n|MAX]`.
    Right(Amount),
    /// `TOP`.
    Top,
    /// `BOTTOM`.
    Bottom,
    /// `NEXT`: to the next breakpoint.
    Next,
    /// `PREVIOUS`: to the previous breakpoint.
    Previous,
    /// `HELP`.
    Help,
    /// `SET option value`.
    Set(SetOption),
    /// `THREAD`: Work with Debugged Threads.
    Thread,
    /// `EQUATE name [definition]`; no definition removes the name.
    Equate {
        /// The name.
        name: String,
        /// What it stands for.
        def: Option<String>,
    },
}

/// The commands with their shortest abbreviation's length.
pub const WORDS: &[(&str, usize)] = &[
    ("ATTR", 1),
    ("BREAK", 2),
    ("BOTTOM", 2),
    ("CLEAR", 1),
    ("DISPLAY", 2),
    ("DOWN", 2),
    ("EQUATE", 1),
    ("EVAL", 2),
    ("FIND", 1),
    ("HELP", 1),
    ("LEFT", 1),
    ("NEXT", 1),
    ("PREVIOUS", 1),
    ("QUAL", 1),
    ("RIGHT", 1),
    ("SET", 2),
    ("STEP", 1),
    ("TBREAK", 2),
    ("THREAD", 2),
    ("TOP", 1),
    ("UP", 1),
    ("WATCH", 1),
];

/// The command word `w` abbreviates, if any.
#[must_use]
pub fn word_of(w: &str) -> Option<&'static str> {
    let w = w.to_ascii_uppercase();
    WORDS
        .iter()
        .find(|(name, min)| w.len() >= *min && name.starts_with(w.as_str()))
        .map(|(name, _)| *name)
}

/// The first word and the rest of a line.
fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    }
}

fn amount(rest: &str) -> Result<Amount, DbgError> {
    let r = rest.trim();
    if r.is_empty() {
        Ok(Amount::Default)
    } else if r.eq_ignore_ascii_case("MAX") {
        Ok(Amount::Max)
    } else {
        r.parse()
            .map(Amount::By)
            .map_err(|_| DbgError::Operand(r.to_string()))
    }
}

fn number(s: &str) -> Result<usize, DbgError> {
    s.trim()
        .parse()
        .map_err(|_| DbgError::Operand(s.trim().to_string()))
}

/// A breakpoint's operands: `[place] [WHEN expr]`.
fn break_parts(rest: &str) -> Result<(Place, Option<String>), DbgError> {
    let upper = rest.to_ascii_uppercase();
    let (at, when) = match find_word(&upper, "WHEN") {
        Some(i) => (&rest[..i], Some(rest[i + 4..].trim().to_string())),
        None => (rest, None),
    };
    if when.as_deref() == Some("") {
        return Err(DbgError::Missing("WHEN"));
    }
    Ok((place(at.trim())?, when))
}

/// Where a whole word starts in an upper-case line.
fn find_word(upper: &str, word: &str) -> Option<usize> {
    let b = upper.as_bytes();
    upper.match_indices(word).map(|(i, _)| i).find(|&i| {
        let before = i == 0 || b[i - 1].is_ascii_whitespace();
        let after = b.get(i + word.len()).is_none_or(u8::is_ascii_whitespace);
        before && after
    })
}

fn place(s: &str) -> Result<Place, DbgError> {
    if s.is_empty() {
        return Ok(Place::Cursor);
    }
    if let Some((proc, line)) = s.split_once('/') {
        return Ok(Place::ProcLine(
            proc.trim().to_ascii_uppercase(),
            number(line)?,
        ));
    }
    number(s).map(Place::Line)
}

/// `expr [: X|C [n]]`.
fn eval_parts(rest: &str) -> (String, Format, Option<usize>) {
    if let Some(i) = rest.rfind(':') {
        let tail = rest[i + 1..].trim();
        let (f, n) = split(tail);
        let format = match f.to_ascii_uppercase().as_str() {
            "X" => Some(Format::Hex),
            "C" => Some(Format::Char),
            _ => None,
        };
        if let Some(format) = format {
            let len = n.parse().ok();
            return (rest[..i].trim().to_string(), format, len);
        }
    }
    (rest.trim().to_string(), Format::Natural, None)
}

fn yes_no(v: &str) -> Result<bool, DbgError> {
    match v.to_ascii_uppercase().as_str() {
        "YES" | "Y" | "MATCH" => Ok(true),
        "NO" | "N" | "NOMATCH" => Ok(false),
        _ => Err(DbgError::Operand(v.to_string())),
    }
}

/// Parse one command line.
#[allow(clippy::too_many_lines)] // one arm per command
pub fn parse(line: &str) -> Result<Command, DbgError> {
    let (w, rest) = split(line);
    if w.is_empty() {
        return Err(DbgError::Empty);
    }
    let word = word_of(w).ok_or_else(|| DbgError::Unknown(w.to_ascii_uppercase()))?;
    let upper = rest.to_ascii_uppercase();
    Ok(match word {
        "BREAK" => {
            let (at, when) = break_parts(rest)?;
            Command::Break { at, when }
        }
        "TBREAK" => {
            let (at, when) = break_parts(rest)?;
            Command::TBreak { at, when }
        }
        "CLEAR" => Command::Clear(clear(rest, &upper)?),
        "DISPLAY" => {
            let (what, arg) = split(&upper);
            match what {
                "" | "BREAKPOINTS" | "BR" => Command::Display(DisplayWhat::Breakpoints),
                "EQUATE" | "EQ" => Command::Display(DisplayWhat::Equates),
                "WATCH" | "WA" => Command::Display(DisplayWhat::Watch),
                "MODULE" | "MO" if !arg.is_empty() => {
                    Command::Display(DisplayWhat::Module(arg.to_string()))
                }
                "MODULE" | "MO" => return Err(DbgError::Missing("MODULE")),
                other => return Err(DbgError::Operand(other.to_string())),
            }
        }
        "EVAL" => {
            if rest.is_empty() {
                return Err(DbgError::Missing("EVAL"));
            }
            if let Some((target, value)) = assignment(rest) {
                Command::Eval {
                    expr: target,
                    assign: Some(value),
                    format: Format::Natural,
                    len: None,
                }
            } else {
                let (expr, format, len) = eval_parts(rest);
                Command::Eval {
                    expr,
                    assign: None,
                    format,
                    len,
                }
            }
        }
        "WATCH" => {
            if rest.is_empty() {
                return Err(DbgError::Missing("WATCH"));
            }
            let (expr, len) = match rest.rsplit_once(':') {
                Some((e, n)) => (e.trim().to_string(), Some(number(n)?)),
                None => (rest.to_string(), None),
            };
            Command::Watch { expr, len }
        }
        "STEP" => step(&upper)?,
        "ATTR" => {
            if rest.is_empty() {
                return Err(DbgError::Missing("ATTR"));
            }
            Command::Attr(rest.to_string())
        }
        "QUAL" => Command::Qual(number(rest)?),
        "FIND" => find(rest),
        "UP" => Command::Up(amount(rest)?),
        "DOWN" => Command::Down(amount(rest)?),
        "LEFT" => Command::Left(amount(rest)?),
        "RIGHT" => Command::Right(amount(rest)?),
        "TOP" => Command::Top,
        "BOTTOM" => Command::Bottom,
        "NEXT" => Command::Next,
        "PREVIOUS" => Command::Previous,
        "HELP" => Command::Help,
        "THREAD" => Command::Thread,
        "SET" => {
            let (opt, v) = split(&upper);
            match opt {
                "UPDPROD" => Command::Set(SetOption::UpdProd(yes_no(v)?)),
                "CASE" => Command::Set(SetOption::Case(yes_no(v)?)),
                "OPMSRC" => Command::Set(SetOption::OpmSrc(yes_no(v)?)),
                "" => return Err(DbgError::Missing("SET")),
                other => return Err(DbgError::Operand(other.to_string())),
            }
        }
        "EQUATE" => {
            let (name, def) = split(rest);
            if name.is_empty() {
                return Err(DbgError::Missing("EQUATE"));
            }
            Command::Equate {
                name: name.to_ascii_uppercase(),
                def: Some(def.to_string()).filter(|d| !d.is_empty()),
            }
        }
        _ => unreachable!("every word in WORDS is handled"),
    })
}

/// `target = value`, when the `=` is an assignment (not `==` or part
/// of a comparison).
fn assignment(rest: &str) -> Option<(String, String)> {
    let b = rest.as_bytes();
    let mut quote = false;
    for (i, c) in b.iter().enumerate() {
        match c {
            b'\'' => quote = !quote,
            b'=' if !quote => {
                let prev = i.checked_sub(1).map(|p| b[p]);
                let next = b.get(i + 1).copied();
                if matches!(prev, Some(b'<' | b'>' | b'=' | b'!')) || next == Some(b'=') {
                    return None;
                }
                let target = rest[..i].trim();
                let value = rest[i + 1..].trim();
                if target.is_empty() || value.is_empty() {
                    return None;
                }
                return Some((target.to_string(), value.to_string()));
            }
            _ => {}
        }
    }
    None
}

fn clear(rest: &str, upper: &str) -> Result<ClearWhat, DbgError> {
    let (what, arg) = split(upper);
    Ok(match what {
        "PGM" | "PROGRAM" => ClearWhat::Pgm,
        "WATCH" | "WA" => match arg {
            "ALL" => ClearWhat::WatchAll,
            "" => return Err(DbgError::Missing("WATCH")),
            n => ClearWhat::Watch(number(n)?),
        },
        _ => ClearWhat::At(place(rest.trim())?),
    })
}

fn step(upper: &str) -> Result<Command, DbgError> {
    let mut count = 1;
    let mut into = false;
    for w in upper.split_whitespace() {
        match w {
            "INTO" => into = true,
            "OVER" => into = false,
            n => count = number(n)?.max(1),
        }
    }
    Ok(Command::Step { count, into })
}

fn find(rest: &str) -> Command {
    let upper = rest.to_ascii_uppercase();
    let (body, back) = if let Some(s) = upper.strip_suffix(" PREVIOUS") {
        (&rest[..s.len()], true)
    } else if let Some(s) = upper.strip_suffix(" NEXT") {
        (&rest[..s.len()], false)
    } else if upper == "PREVIOUS" {
        ("", true)
    } else if upper == "NEXT" {
        ("", false)
    } else {
        (rest, false)
    };
    let body = body.trim();
    let what = body
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .map_or_else(|| body.to_string(), |s| s.replace("''", "'"));
    Command::Find { what, back }
}

/// The session's `EQUATE` names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Equates {
    /// Name to definition.
    pub defs: BTreeMap<String, String>,
}

impl Equates {
    /// Define or, with `None`, remove a name.
    pub fn set(&mut self, name: &str, def: Option<String>) {
        match def {
            Some(d) => {
                self.defs.insert(name.to_ascii_uppercase(), d);
            }
            None => {
                self.defs.remove(&name.to_ascii_uppercase());
            }
        }
    }

    /// The line with a leading equate name replaced by its definition
    /// (the rest of the line follows it).
    #[must_use]
    pub fn expand(&self, line: &str) -> String {
        let (w, rest) = split(line);
        match self.defs.get(&w.to_ascii_uppercase()) {
            Some(d) if rest.is_empty() => d.clone(),
            Some(d) => format!("{d} {rest}"),
            None => line.trim().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviations() {
        assert_eq!(word_of("br"), Some("BREAK"));
        assert_eq!(word_of("b"), None);
        assert_eq!(word_of("bo"), Some("BOTTOM"));
        assert_eq!(word_of("c"), Some("CLEAR"));
        assert_eq!(word_of("di"), Some("DISPLAY"));
        assert_eq!(word_of("d"), None);
        assert_eq!(word_of("ev"), Some("EVAL"));
        assert_eq!(word_of("e"), Some("EQUATE"));
        assert_eq!(word_of("s"), Some("STEP"));
        assert_eq!(word_of("se"), Some("SET"));
        assert_eq!(word_of("tb"), Some("TBREAK"));
        assert_eq!(word_of("th"), Some("THREAD"));
        assert_eq!(word_of("t"), Some("TOP"));
        assert_eq!(word_of("breakx"), None);
    }

    #[test]
    fn breaks() {
        assert_eq!(
            parse("break 12").unwrap(),
            Command::Break {
                at: Place::Line(12),
                when: None
            }
        );
        assert_eq!(
            parse("BR 12 when &X = 5").unwrap(),
            Command::Break {
                at: Place::Line(12),
                when: Some("&X = 5".into())
            }
        );
        assert_eq!(
            parse("break").unwrap(),
            Command::Break {
                at: Place::Cursor,
                when: None
            }
        );
        assert_eq!(
            parse("tbreak calc/7").unwrap(),
            Command::TBreak {
                at: Place::ProcLine("CALC".into(), 7),
                when: None
            }
        );
        assert!(parse("break x").is_err());
        assert!(parse("break 4 when").is_err());
    }

    #[test]
    fn clears() {
        assert_eq!(parse("clear pgm").unwrap(), Command::Clear(ClearWhat::Pgm));
        assert_eq!(
            parse("c 9").unwrap(),
            Command::Clear(ClearWhat::At(Place::Line(9)))
        );
        assert_eq!(
            parse("clear watch all").unwrap(),
            Command::Clear(ClearWhat::WatchAll)
        );
        assert_eq!(
            parse("clear watch 2").unwrap(),
            Command::Clear(ClearWhat::Watch(2))
        );
    }

    #[test]
    fn evals() {
        assert_eq!(
            parse("eval &count").unwrap(),
            Command::Eval {
                expr: "&count".into(),
                assign: None,
                format: Format::Natural,
                len: None
            }
        );
        assert_eq!(
            parse("ev name : x 16").unwrap(),
            Command::Eval {
                expr: "name".into(),
                assign: None,
                format: Format::Hex,
                len: Some(16)
            }
        );
        assert_eq!(
            parse("eval x = 'a=b'").unwrap(),
            Command::Eval {
                expr: "x".into(),
                assign: Some("'a=b'".into()),
                format: Format::Natural,
                len: None
            }
        );
        assert!(matches!(
            parse("eval x >= 2").unwrap(),
            Command::Eval { assign: None, .. }
        ));
        assert!(parse("eval").is_err());
    }

    #[test]
    fn steps_and_scrolls() {
        assert_eq!(
            parse("step").unwrap(),
            Command::Step {
                count: 1,
                into: false
            }
        );
        assert_eq!(
            parse("s 3 into").unwrap(),
            Command::Step {
                count: 3,
                into: true
            }
        );
        assert_eq!(parse("up").unwrap(), Command::Up(Amount::Default));
        assert_eq!(parse("down 5").unwrap(), Command::Down(Amount::By(5)));
        assert_eq!(parse("right max").unwrap(), Command::Right(Amount::Max));
        assert_eq!(parse("l 2").unwrap(), Command::Left(Amount::By(2)));
        assert_eq!(parse("top").unwrap(), Command::Top);
        assert_eq!(parse("bottom").unwrap(), Command::Bottom);
        assert_eq!(parse("n").unwrap(), Command::Next);
        assert_eq!(parse("p").unwrap(), Command::Previous);
    }

    #[test]
    fn the_rest() {
        assert_eq!(
            parse("find 'call pgm' previous").unwrap(),
            Command::Find {
                what: "call pgm".into(),
                back: true
            }
        );
        assert_eq!(
            parse("f next").unwrap(),
            Command::Find {
                what: String::new(),
                back: false
            }
        );
        assert_eq!(parse("qual 14").unwrap(), Command::Qual(14));
        assert_eq!(parse("attr &x").unwrap(), Command::Attr("&x".into()));
        assert_eq!(
            parse("watch &total : 4").unwrap(),
            Command::Watch {
                expr: "&total".into(),
                len: Some(4)
            }
        );
        assert_eq!(
            parse("set updprod yes").unwrap(),
            Command::Set(SetOption::UpdProd(true))
        );
        assert_eq!(
            parse("set case nomatch").unwrap(),
            Command::Set(SetOption::Case(false))
        );
        assert_eq!(parse("help").unwrap(), Command::Help);
        assert_eq!(parse("thread").unwrap(), Command::Thread);
        assert_eq!(
            parse("display module calc").unwrap(),
            Command::Display(DisplayWhat::Module("CALC".into()))
        );
        assert_eq!(
            parse("display").unwrap(),
            Command::Display(DisplayWhat::Breakpoints)
        );
        assert!(matches!(parse("frob"), Err(DbgError::Unknown(_))));
        assert!(matches!(parse("  "), Err(DbgError::Empty)));
    }

    #[test]
    fn equates() {
        let mut e = Equates::default();
        assert_eq!(
            parse("equate sm eval &count").unwrap(),
            Command::Equate {
                name: "SM".into(),
                def: Some("eval &count".into())
            }
        );
        e.set("sm", Some("eval &count".into()));
        assert_eq!(e.expand("sm"), "eval &count");
        assert_eq!(e.expand("SM : x"), "eval &count : x");
        assert_eq!(e.expand("step"), "step");
        e.set("SM", None);
        assert_eq!(e.expand("sm"), "sm");
    }
}
