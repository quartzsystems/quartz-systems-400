// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The line commands typed in the `CMD` area, as IBM's EDTF has them:
//! `I`/`In` insert, `D`/`Dn`/`DD` delete, `C`/`Cn`/`CC` copy and
//! `M`/`Mn`/`MM` move to an `A` (after) or `B` (before) target, `R`/`Rn`/
//! `RR` (also `"`/`"n`/`""`) repeat, `<`/`<n`/`<<` and `>`/`>n`/`>>`
//! shift (also `LT n`, `RT n`). A block's two ends, or a copy and its
//! target, may be typed on different screens: what is incomplete waits.

use crate::buffer::Buffer;

/// One command as typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    /// Insert `n` lines after.
    Insert(usize),
    /// Delete `n` lines.
    Delete(usize),
    /// One end of a delete block.
    DeleteBlock,
    /// Copy `n` lines.
    Copy(usize),
    /// One end of a copy block.
    CopyBlock,
    /// Move `n` lines.
    Move(usize),
    /// One end of a move block.
    MoveBlock,
    /// Repeat the line `n` times.
    Repeat(usize),
    /// One end of a repeat block.
    RepeatBlock,
    /// Shift `n` columns left (data past column 1 is lost).
    Left(usize),
    /// One end of a left shift block, by `n`.
    LeftBlock(usize),
    /// Shift `n` columns right.
    Right(usize),
    /// One end of a right shift block, by `n`.
    RightBlock(usize),
    /// The target: after this line.
    After,
    /// The target: before this line.
    Before,
}

impl Cmd {
    /// The kind of block this is one end of, if it is one.
    fn block(self) -> Option<u8> {
        match self {
            Self::DeleteBlock => Some(b'D'),
            Self::CopyBlock => Some(b'C'),
            Self::MoveBlock => Some(b'M'),
            Self::RepeatBlock => Some(b'R'),
            Self::LeftBlock(_) => Some(b'<'),
            Self::RightBlock(_) => Some(b'>'),
            _ => None,
        }
    }

    fn is_target(self) -> bool {
        matches!(self, Self::After | Self::Before)
    }

    fn needs_target(self) -> bool {
        matches!(self, Self::Copy(_) | Self::Move(_) | Self::CopyBlock | Self::MoveBlock)
    }
}

/// A count: empty is 1.
fn count(s: &str) -> Option<usize> {
    let s = s.trim();
    if s.is_empty() {
        return Some(1);
    }
    s.parse::<usize>().ok().filter(|n| *n > 0)
}

/// Parse a typed command; `None` when it is not one.
#[must_use]
pub fn parse(typed: &str) -> Option<Cmd> {
    let t = typed.trim().to_ascii_uppercase();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.strip_prefix("LT") {
        return count(rest).map(Cmd::Left);
    }
    if let Some(rest) = t.strip_prefix("RT") {
        return count(rest).map(Cmd::Right);
    }
    match t.as_str() {
        "DD" => return Some(Cmd::DeleteBlock),
        "CC" => return Some(Cmd::CopyBlock),
        "MM" => return Some(Cmd::MoveBlock),
        "RR" | "\"\"" => return Some(Cmd::RepeatBlock),
        "A" => return Some(Cmd::After),
        "B" => return Some(Cmd::Before),
        _ => {}
    }
    if let Some(rest) = t.strip_prefix("<<") {
        return count(rest).map(Cmd::LeftBlock);
    }
    if let Some(rest) = t.strip_prefix(">>") {
        return count(rest).map(Cmd::RightBlock);
    }
    let mut chars = t.chars();
    let head = chars.next()?;
    let n = count(chars.as_str())?;
    Some(match head {
        'I' => Cmd::Insert(n),
        'D' => Cmd::Delete(n),
        'C' => Cmd::Copy(n),
        'M' => Cmd::Move(n),
        'R' | '"' => Cmd::Repeat(n),
        '<' => Cmd::Left(n),
        '>' => Cmd::Right(n),
        _ => return None,
    })
}

/// A command on a line (0 is the top of the file, the Beginning of data
/// line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    /// The line's number.
    pub id: u64,
    /// The command.
    pub cmd: Cmd,
}

/// What applying the commands did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    /// Commands still waiting (a block end, a copy's target).
    pub pending: Vec<Placed>,
    /// The first line an insert made, for the cursor.
    pub inserted: Option<u64>,
    /// A copy or move is waiting for its target.
    pub needs_target: bool,
}

/// The index range of lines `a` and `b` (in either order), inclusive.
fn span(buf: &Buffer, a: u64, b: u64) -> Option<(usize, usize)> {
    let (x, y) = (buf.index_of(a)?, buf.index_of(b)?);
    Some((x.min(y), x.max(y)))
}

fn shift(text: &str, n: usize, left: bool) -> String {
    if left {
        text.chars().skip(n).collect()
    } else {
        format!("{}{text}", " ".repeat(n))
    }
}

/// Apply `new` (this screen's commands, top to bottom) after what was
/// `pending`.
pub fn apply(buf: &mut Buffer, pending: Vec<Placed>, new: Vec<Placed>) -> Applied {
    let mut all = pending;
    all.extend(new);
    // Pair up block ends of the same kind, in order typed.
    let mut blocks: Vec<(Cmd, u64, u64)> = Vec::new();
    let mut singles: Vec<Placed> = Vec::new();
    let mut open: Vec<Placed> = Vec::new();
    for p in all {
        if let Some(k) = p.cmd.block() {
            if let Some(i) = open.iter().position(|o| o.cmd.block() == Some(k)) {
                let o = open.remove(i);
                let cmd = match (o.cmd, p.cmd) {
                    (Cmd::LeftBlock(a), Cmd::LeftBlock(b)) => Cmd::LeftBlock(a.max(b)),
                    (Cmd::RightBlock(a), Cmd::RightBlock(b)) => Cmd::RightBlock(a.max(b)),
                    (c, _) => c,
                };
                blocks.push((cmd, o.id, p.id));
            } else {
                open.push(p);
            }
        } else {
            singles.push(p);
        }
    }
    let mut out = Applied::default();
    let target = singles.iter().copied().find(|p| p.cmd.is_target());
    let wants_target = blocks.iter().any(|(c, _, _)| c.needs_target())
        || singles.iter().any(|p| p.cmd.needs_target());
    if wants_target && target.is_none() {
        // Everything waits for the target.
        out.pending = open;
        out.pending.extend(singles.iter().copied().filter(|p| p.cmd.needs_target()));
        for (c, a, b) in &blocks {
            out.pending.push(Placed { id: *a, cmd: *c });
            out.pending.push(Placed { id: *b, cmd: *c });
        }
        out.needs_target = true;
        apply_simple(buf, &singles, &mut out);
        return out;
    }
    out.pending = open;
    if !wants_target
        && let Some(t) = target {
            // A target with nothing to copy waits for its copy.
            out.pending.push(t);
        }
    // Copies and moves first, by their source lines.
    if let Some(t) = target.filter(|_| wants_target) {
        let mut src: Option<(Cmd, usize, usize)> = None;
        for (c, a, b) in &blocks {
            if c.needs_target()
                && let Some((x, y)) = span(buf, *a, *b) {
                    src = Some((*c, x, y + 1));
                }
        }
        for p in &singles {
            if let (Cmd::Copy(n) | Cmd::Move(n), Some(i)) = (p.cmd, buf.index_of(p.id)) {
                src = Some((p.cmd, i, (i + n).min(buf.len())));
            }
        }
        if let Some((c, from, to)) = src {
            let texts: Vec<String> = buf.lines()[from..to].iter().map(|l| l.text.clone()).collect();
            let moving = matches!(c, Cmd::Move(_) | Cmd::MoveBlock);
            if moving {
                buf.remove(from, to);
            }
            let at = if t.id == 0 {
                0
            } else {
                match buf.index_of(t.id) {
                    Some(i) if t.cmd == Cmd::After => i + 1,
                    Some(i) => i,
                    // The target was among the lines moved: put them back.
                    None => from,
                }
            };
            buf.insert(at, texts);
        }
    }
    for (c, a, b) in &blocks {
        let Some((x, y)) = span(buf, *a, *b) else {
            continue;
        };
        match c {
            Cmd::DeleteBlock => {
                buf.remove(x, y + 1);
            }
            Cmd::RepeatBlock => {
                let texts: Vec<String> = buf.lines()[x..=y].iter().map(|l| l.text.clone()).collect();
                buf.insert(y + 1, texts);
            }
            Cmd::LeftBlock(n) | Cmd::RightBlock(n) => {
                for i in x..=y {
                    let t = shift(buf.text(i), *n, matches!(c, Cmd::LeftBlock(_)));
                    buf.set(i, t);
                }
            }
            _ => {}
        }
    }
    apply_simple(buf, &singles, &mut out);
    out
}

/// The commands that need nothing else: insert, delete, repeat, shift.
fn apply_simple(buf: &mut Buffer, singles: &[Placed], out: &mut Applied) {
    for p in singles {
        let idx = if p.id == 0 { None } else { buf.index_of(p.id) };
        if p.id != 0 && idx.is_none() {
            continue;
        }
        match p.cmd {
            Cmd::Insert(n) => {
                let at = idx.map_or(0, |i| i + 1);
                let ids = buf.insert_fresh(at, n);
                out.inserted = out.inserted.or(ids.first().copied());
            }
            Cmd::Delete(n) => {
                if let Some(i) = idx {
                    buf.remove(i, i + n);
                }
            }
            Cmd::Repeat(n) => {
                if let Some(i) = idx {
                    let t = buf.text(i).to_string();
                    buf.insert(i + 1, vec![t; n]);
                }
            }
            Cmd::Left(n) | Cmd::Right(n) => {
                if let Some(i) = idx {
                    let t = shift(buf.text(i), n, matches!(p.cmd, Cmd::Left(_)));
                    buf.set(i, t);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf() -> Buffer {
        Buffer::new(["a", "b", "c", "d"].iter().map(|s| (*s).to_string()).collect())
    }

    fn at(b: &Buffer, i: usize, cmd: &str) -> Placed {
        Placed {
            id: b.lines()[i].id,
            cmd: parse(cmd).unwrap(),
        }
    }

    #[test]
    fn parses() {
        assert_eq!(parse("i3"), Some(Cmd::Insert(3)));
        assert_eq!(parse("dd"), Some(Cmd::DeleteBlock));
        assert_eq!(parse("\"2"), Some(Cmd::Repeat(2)));
        assert_eq!(parse("<<4"), Some(Cmd::LeftBlock(4)));
        assert_eq!(parse("LT 2"), Some(Cmd::Left(2)));
        assert_eq!(parse("x"), None);
        assert_eq!(parse("d0"), None);
    }

    #[test]
    fn insert_delete_repeat() {
        let mut b = buf();
        let p = vec![at(&b, 0, "I2"), at(&b, 3, "D")];
        let r = apply(&mut b, Vec::new(), p);
        assert!(r.inserted.is_some());
        assert_eq!(b.texts(), vec!["a", "", "", "b", "c"]);
        b.drop_fresh(0);
        assert_eq!(b.texts(), vec!["a", "b", "c"]);
        let p = vec![at(&b, 1, "R2")];
        apply(&mut b, Vec::new(), p);
        assert_eq!(b.texts(), vec!["a", "b", "b", "b", "c"]);
    }

    #[test]
    fn blocks_across_screens() {
        let mut b = buf();
        let first = vec![at(&b, 1, "DD")];
        let r = apply(&mut b, Vec::new(), first);
        assert_eq!(r.pending.len(), 1);
        let second = vec![at(&b, 2, "DD")];
        let r = apply(&mut b, r.pending, second);
        assert!(r.pending.is_empty());
        assert_eq!(b.texts(), vec!["a", "d"]);
    }

    #[test]
    fn copy_and_move() {
        let mut b = buf();
        let p = vec![at(&b, 0, "C")];
        let r = apply(&mut b, Vec::new(), p);
        assert!(r.needs_target);
        let p = vec![at(&b, 3, "A")];
        let r = apply(&mut b, r.pending, p);
        assert!(r.pending.is_empty());
        assert_eq!(b.texts(), vec!["a", "b", "c", "d", "a"]);
        let mut b = buf();
        let p = vec![at(&b, 2, "MM"), at(&b, 3, "MM"), Placed { id: 0, cmd: Cmd::After }];
        apply(&mut b, Vec::new(), p);
        assert_eq!(b.texts(), vec!["c", "d", "a", "b"]);
        let mut b = buf();
        let p = vec![at(&b, 0, "M"), at(&b, 2, "B")];
        apply(&mut b, Vec::new(), p);
        assert_eq!(b.texts(), vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn shifts() {
        let mut b = Buffer::new(vec!["  x".into(), "  y".into()]);
        let p = vec![at(&b, 0, "<<2"), at(&b, 1, "<<")];
        apply(&mut b, Vec::new(), p);
        assert_eq!(b.texts(), vec!["x", "y"]);
        let p = vec![at(&b, 1, ">3")];
        apply(&mut b, Vec::new(), p);
        assert_eq!(b.texts(), vec!["x", "   y"]);
    }
}
