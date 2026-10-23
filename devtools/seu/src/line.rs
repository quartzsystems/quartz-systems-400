// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Line commands: what the operator typed over a sequence number, read
//! back into a command, and the rules that pair sources with targets
//! and block ends with block ends. A command whose partner is not on
//! the display yet stays pending until it comes.

/// What a line command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `A`: the target is after this line.
    After,
    /// `B`: the target is before this line.
    Before,
    /// `O`, `On`, `OO`: the target lines are overlaid.
    Overlay,
    /// `C`, `Cn`, `CC`: copy.
    Copy,
    /// `CR`, `CRn`, `CCR`: copy and keep the source pending.
    CopyRepeat,
    /// `M`, `Mn`, `MM`: move.
    Move,
    /// `D`, `Dn`, `DD`: delete.
    Delete,
    /// `I`, `In`: insert blank lines.
    Insert,
    /// `IP`, `IPxx`: insert a line and prompt it.
    InsertPrompt,
    /// `IF`, `IFxx`: insert a line under a format line.
    InsertFormat,
    /// `RP`, `RPn`, `RPP`: repeat.
    Repeat,
    /// `L`, `Ln`, `LL`: shift the data left.
    ShiftLeft,
    /// `R`, `Rn`, `RR`: shift the data right.
    ShiftRight,
    /// `X`, `Xn`, `XX`: exclude from the display.
    Exclude,
    /// `S`, `Sn`, `SS`: show excluded lines.
    Show,
    /// `SF`, `SFn`: show the first excluded lines.
    ShowFirst,
    /// `SL`, `SLn`: show the last excluded lines.
    ShowLast,
    /// `F`, `Fn`, `FF`, `Fxx`: show a format line.
    Format,
    /// `P`, `Pn`, `PP`, `Pxx`: prompt.
    Prompt,
    /// `W`, `Wn`, `WW`: the window starts at column n.
    Window,
    /// `T`, `Tn`, `TT`: this line to the top of the display.
    Top,
    /// `+`, `+n`: roll forward n lines.
    Forward,
    /// `-`, `-n`: roll back n lines.
    Back,
    /// A sequence number: that line to the top.
    Position,
}

impl Kind {
    /// A source that needs a target.
    #[must_use]
    pub fn is_source(self) -> bool {
        matches!(self, Self::Copy | Self::CopyRepeat | Self::Move)
    }

    /// A target that needs a source.
    #[must_use]
    pub fn is_target(self) -> bool {
        matches!(self, Self::After | Self::Before | Self::Overlay)
    }
}

/// One line command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineCmd {
    /// What it does.
    pub kind: Kind,
    /// Its number: lines, positions, a column, a sequence number.
    pub count: Option<u32>,
    /// One end of a block (`CC`, `DD`, `RPP`, ...).
    pub block: bool,
    /// A format or prompt name (`Fxx`, `IPxx`, `Pxx`).
    pub format: Option<String>,
    /// The text as typed, upper-cased, as a pending command shows it.
    pub text: String,
}

/// Why a typed command is no command.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LineError {
    /// Not a line command SEU knows.
    #[error("line command {0} is not valid")]
    Unknown(String),
}

/// The command typed over a sequence field: the characters up to the
/// last one that differs from what was shown, blanks trimmed. `None`
/// when nothing changed.
#[must_use]
pub fn typed(shown: &str, now: &str) -> Option<String> {
    let s: Vec<char> = shown.chars().collect();
    let n: Vec<char> = now.chars().collect();
    let len = s.len().max(n.len());
    let last = (0..len)
        .rev()
        .find(|&i| s.get(i).copied().unwrap_or(' ') != n.get(i).copied().unwrap_or(' '))?;
    let cmd: String = n.iter().take(last + 1).collect();
    let cmd = cmd.trim().to_string();
    (!cmd.is_empty()).then_some(cmd)
}

/// Read a command. `+`/`-` roll, digits position, letters are the
/// commands with their optional number and format name.
pub fn parse(text: &str) -> Result<LineCmd, LineError> {
    let t = text.trim().to_ascii_uppercase();
    let bad = || LineError::Unknown(t.clone());
    let mk = |kind, count, block, format: Option<String>| LineCmd {
        kind,
        count,
        block,
        format,
        text: t.clone(),
    };
    if t.is_empty() {
        return Err(bad());
    }
    if let Some(rest) = t.strip_prefix('+').or_else(|| t.strip_prefix('-')) {
        let kind = if t.starts_with('+') {
            Kind::Forward
        } else {
            Kind::Back
        };
        let n = if rest.is_empty() {
            None
        } else {
            Some(rest.parse::<u32>().map_err(|_| bad())?)
        };
        return Ok(mk(kind, n, false, None));
    }
    if t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        let seq = crate::parse_seq(&t).ok_or_else(bad)?;
        return Ok(mk(Kind::Position, Some(seq), false, None));
    }
    // The letters, then a number, then (for formats) more letters.
    let letters: String = t.chars().take_while(char::is_ascii_alphabetic).collect();
    let rest = &t[letters.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let tail = &rest[digits.len()..];
    let count = if digits.is_empty() {
        None
    } else {
        Some(digits.parse::<u32>().map_err(|_| bad())?)
    };
    let plain = |kind| {
        if tail.is_empty() {
            Ok(mk(kind, count, false, None))
        } else {
            Err(bad())
        }
    };
    let block = |kind| {
        if tail.is_empty() {
            Ok(mk(kind, count, true, None))
        } else {
            Err(bad())
        }
    };
    let fmt = |kind, name: &str| {
        if !tail.is_empty() || name.len() > 2 {
            return Err(bad());
        }
        let f = (!name.is_empty()).then(|| name.to_string());
        Ok(mk(kind, count, false, f))
    };
    match letters.as_str() {
        "A" => plain(Kind::After),
        "B" => plain(Kind::Before),
        "O" => plain(Kind::Overlay),
        "OO" => block(Kind::Overlay),
        "C" => plain(Kind::Copy),
        "CC" => block(Kind::Copy),
        "CR" => plain(Kind::CopyRepeat),
        "CCR" => block(Kind::CopyRepeat),
        "M" => plain(Kind::Move),
        "MM" => block(Kind::Move),
        "D" => plain(Kind::Delete),
        "DD" => block(Kind::Delete),
        "I" => plain(Kind::Insert),
        "RP" => plain(Kind::Repeat),
        "RPP" => block(Kind::Repeat),
        "L" => plain(Kind::ShiftLeft),
        "LL" => block(Kind::ShiftLeft),
        "R" => plain(Kind::ShiftRight),
        "RR" => block(Kind::ShiftRight),
        "X" => plain(Kind::Exclude),
        "XX" => block(Kind::Exclude),
        "S" => plain(Kind::Show),
        "SS" => block(Kind::Show),
        "SF" => plain(Kind::ShowFirst),
        "SL" => plain(Kind::ShowLast),
        "W" | "WW" => plain(Kind::Window),
        "T" | "TT" => plain(Kind::Top),
        "PP" => block(Kind::Prompt),
        "FF" => plain(Kind::Format),
        l if l.starts_with("IP") => fmt(Kind::InsertPrompt, &l[2..]),
        l if l.starts_with("IF") => fmt(Kind::InsertFormat, &l[2..]),
        l if l.starts_with('F') => fmt(Kind::Format, &l[1..]),
        l if l.starts_with('P') => fmt(Kind::Prompt, &l[1..]),
        _ => Err(bad()),
    }
}

/// A command on a line: the record's identifier (or the display line
/// it sits on when it is no record) and the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The record.
    pub id: u64,
    /// The command.
    pub cmd: LineCmd,
}

/// What a set of commands comes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Commands complete in themselves, in display order.
    pub single: Vec<Placed>,
    /// Blocks: the kind, the first and last record, the first end's
    /// command (for its number and text).
    pub blocks: Vec<(Kind, u64, u64, LineCmd)>,
    /// A copy or move: the source (first and last record, the
    /// command) and the target (record, command).
    pub transfer: Option<Transfer>,
    /// What waits for a partner.
    pub pending: Vec<Placed>,
}

/// A copy or move with its source and target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// Copy, copy-repeat or move.
    pub kind: Kind,
    /// First source record.
    pub from: u64,
    /// Last source record (the same as `from` for one line).
    pub to: u64,
    /// Lines from `from` when the source was `Cn`/`Mn`.
    pub count: Option<u32>,
    /// The target record.
    pub target: u64,
    /// After, before or overlay.
    pub target_kind: Kind,
    /// Overlay lines from the target (`On`), or the block's last
    /// record (`OO`).
    pub target_count: Option<u32>,
    /// The overlay block's last record.
    pub target_to: Option<u64>,
    /// The source as typed, for copy-repeat to stay pending.
    pub source_cmd: LineCmd,
}

/// Why a set of commands cannot be done.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// Two sources, or two targets.
    #[error("more than one {0} command")]
    Conflict(&'static str),
    /// A block was opened by one command and closed by another.
    #[error("block command {0} does not match {1}")]
    Mismatch(String, String),
}

/// Pair the commands (display order: the pending ones from before are
/// merged in by the caller, each at its record's place). Blocks pair
/// their two ends; a source pairs with a target.
pub fn plan(cmds: &[Placed]) -> Result<Plan, PlanError> {
    let mut p = Plan::default();
    let mut open: Option<&Placed> = None;
    let mut sources: Vec<(u64, u64, LineCmd)> = Vec::new();
    let mut targets: Vec<(u64, Option<u64>, LineCmd)> = Vec::new();
    for c in cmds {
        if c.cmd.block {
            match open.take() {
                None => open = Some(c),
                Some(first) if first.cmd.kind == c.cmd.kind => {
                    let k = c.cmd.kind;
                    if k.is_source() {
                        sources.push((first.id, c.id, first.cmd.clone()));
                    } else if k.is_target() {
                        targets.push((first.id, Some(c.id), first.cmd.clone()));
                    } else {
                        p.blocks.push((k, first.id, c.id, first.cmd.clone()));
                    }
                }
                Some(first) => {
                    return Err(PlanError::Mismatch(first.cmd.text.clone(), c.cmd.text.clone()));
                }
            }
            continue;
        }
        if c.cmd.kind.is_source() {
            sources.push((c.id, c.id, c.cmd.clone()));
        } else if c.cmd.kind.is_target() {
            targets.push((c.id, None, c.cmd.clone()));
        } else {
            p.single.push(c.clone());
        }
    }
    if let Some(first) = open {
        p.pending.push(first.clone());
    }
    if sources.len() > 1 {
        return Err(PlanError::Conflict("source"));
    }
    if targets.len() > 1 {
        return Err(PlanError::Conflict("target"));
    }
    match (sources.pop(), targets.pop()) {
        (Some((from, to, scmd)), Some((target, target_to, tcmd))) => {
            p.transfer = Some(Transfer {
                kind: scmd.kind,
                from,
                to,
                count: if scmd.block { None } else { scmd.count },
                target,
                target_kind: tcmd.kind,
                target_count: if tcmd.block { None } else { tcmd.count },
                target_to,
                source_cmd: scmd,
            });
        }
        (Some((from, to, scmd)), None) => {
            p.pending.push(Placed { id: from, cmd: scmd.clone() });
            if scmd.block {
                p.pending.push(Placed { id: to, cmd: scmd });
            }
        }
        (None, Some((target, target_to, tcmd))) => {
            p.pending.push(Placed {
                id: target,
                cmd: tcmd.clone(),
            });
            if let Some(t) = target_to {
                p.pending.push(Placed { id: t, cmd: tcmd });
            }
        }
        (None, None) => {}
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: u64, t: &str) -> Placed {
        Placed {
            id,
            cmd: parse(t).unwrap(),
        }
    }

    #[test]
    fn typed_over_the_sequence() {
        assert_eq!(typed("0001.00", "i001.00").as_deref(), Some("i"));
        assert_eq!(typed("0001.00", "d301.00").as_deref(), Some("d3"));
        assert_eq!(typed("0001.00", "cc"), Some("cc".into()));
        assert_eq!(typed("0001.00", "0001.00"), None);
        assert_eq!(typed("'''''''", "i''''''").as_deref(), Some("i"));
        assert_eq!(typed("0001.00", "0012.00").as_deref(), Some("0012"));
    }

    #[test]
    fn parses_every_form() {
        let c = parse("d5").unwrap();
        assert_eq!((c.kind, c.count, c.block), (Kind::Delete, Some(5), false));
        assert!(parse("DD").unwrap().block);
        let c = parse("ipc").unwrap();
        assert_eq!((c.kind, c.format.as_deref()), (Kind::InsertPrompt, Some("C")));
        let c = parse("ipcx").unwrap();
        assert_eq!(c.format.as_deref(), Some("CX"));
        assert_eq!(parse("fa").unwrap().kind, Kind::Format);
        assert_eq!(parse("P").unwrap().kind, Kind::Prompt);
        assert!(parse("PP").unwrap().block);
        assert_eq!(parse("+5").unwrap().count, Some(5));
        assert_eq!(parse("-").unwrap().kind, Kind::Back);
        assert_eq!(parse("12").unwrap().count, Some(1200));
        assert_eq!(parse("rpp").unwrap().kind, Kind::Repeat);
        assert_eq!(parse("ccr").unwrap().kind, Kind::CopyRepeat);
        assert_eq!(parse("w20").unwrap().count, Some(20));
        assert_eq!(parse("T").unwrap().kind, Kind::Top);
        assert!(parse("Q").is_err());
        assert!(parse("d5x").is_err());
        assert!(parse("ipabc").is_err());
    }

    #[test]
    fn pairs_blocks() {
        let p = plan(&[at(1, "dd"), at(3, "dd")]).unwrap();
        assert_eq!(p.blocks.len(), 1);
        assert_eq!((p.blocks[0].1, p.blocks[0].2), (1, 3));
        let p = plan(&[at(1, "dd")]).unwrap();
        assert_eq!(p.pending.len(), 1);
        assert!(plan(&[at(1, "dd"), at(2, "xx")]).is_err());
    }

    #[test]
    fn pairs_source_and_target() {
        let p = plan(&[at(1, "cc"), at(2, "cc"), at(5, "a")]).unwrap();
        let t = p.transfer.unwrap();
        assert_eq!((t.from, t.to, t.target, t.target_kind), (1, 2, 5, Kind::After));
        let p = plan(&[at(1, "m")]).unwrap();
        assert!(p.transfer.is_none());
        assert_eq!(p.pending[0].cmd.kind, Kind::Move);
        let p = plan(&[at(4, "b")]).unwrap();
        assert_eq!(p.pending[0].cmd.kind, Kind::Before);
        assert!(plan(&[at(1, "c"), at(2, "m"), at(3, "a")]).is_err());
        let p = plan(&[at(1, "c"), at(4, "oo"), at(6, "oo")]).unwrap();
        assert_eq!(p.transfer.unwrap().target_to, Some(6));
    }

    #[test]
    fn singles_go_through() {
        let p = plan(&[at(1, "d"), at(2, "i3"), at(3, "x")]).unwrap();
        assert_eq!(p.single.len(), 3);
    }
}
