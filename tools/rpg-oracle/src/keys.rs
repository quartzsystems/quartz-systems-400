// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operator's answer to one screen, from a member's `keys:` line:
//! fields typed, the cursor placed, the key pressed. The kit's
//! scripted terminal and the oracle's 5250 session both play it on a
//! `tn5250` terminal, so both screens are read the same way.

use tn5250::term::{Key, KeyResult, Terminal};

/// One thing the operator does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Type into the field at a row and column, then Field Exit
    /// unless `exit` is false.
    Field {
        /// Row, 1-based.
        row: usize,
        /// Column, 1-based.
        col: usize,
        /// What is typed.
        text: String,
        /// Field Exit after it.
        exit: bool,
    },
    /// Put the cursor at a row and column.
    Cursor {
        /// Row, 1-based.
        row: usize,
        /// Column, 1-based.
        col: usize,
    },
    /// Press a key.
    Key(Key),
}

fn pos(s: &str) -> Option<(usize, usize)> {
    let (r, c) = s.split_once(',')?;
    Some((r.trim().parse().ok()?, c.trim().parse().ok()?))
}

/// The words of a step: blanks separate them, `'` quotes a value.
fn words(step: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in step.chars() {
        match ch {
            '\'' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A step's actions; the last is always a key.
pub fn parse(step: &str) -> Result<Vec<Action>, String> {
    let mut out = Vec::new();
    for w in words(step) {
        if let Some(at) = w.strip_prefix('@') {
            let (row, col) = pos(at).ok_or_else(|| format!("{w}: not a row and column"))?;
            out.push(Action::Cursor { row, col });
        } else if let Some((p, text, exit)) = w
            .split_once('=')
            .map(|(p, t)| (p, t, true))
            .or_else(|| w.split_once(':').map(|(p, t)| (p, t, false)))
        {
            let (row, col) = pos(p).ok_or_else(|| format!("{w}: not a row and column"))?;
            out.push(Action::Field {
                row,
                col,
                text: text.to_string(),
                exit,
            });
        } else {
            let k = tn5250_driver::script::key_named(&w).ok_or_else(|| format!("{w}: not a key"))?;
            out.push(Action::Key(k));
        }
    }
    if !matches!(out.last(), Some(Action::Key(_))) {
        out.push(Action::Key(Key::Enter));
    }
    Ok(out)
}

/// Play actions on a terminal, the way the 5250 session plays them:
/// every result but the last key's.
pub fn play(t: &mut Terminal, actions: &[Action]) -> Result<(), String> {
    let check = |r: KeyResult| match r {
        KeyResult::Done | KeyResult::Sent => Ok(()),
        KeyResult::Locked => Err("the keyboard is locked".to_string()),
        KeyResult::Error(code) => Err(format!("operator error {code:04}")),
    };
    for a in actions {
        match a {
            Action::Field { row, col, text, exit } => {
                t.set_cursor(*row, *col);
                check(t.type_str(text))?;
                if *exit {
                    check(t.key(Key::FieldExit))?;
                }
            }
            Action::Cursor { row, col } => t.set_cursor(*row, *col),
            Action::Key(k) => check(t.key(*k))?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_parse() {
        let a = parse("3,20=A100 6,10='TWO WORDS' @10,3 f3").unwrap();
        assert_eq!(a.len(), 4);
        assert_eq!(
            a[1],
            Action::Field {
                row: 6,
                col: 10,
                text: "TWO WORDS".into(),
                exit: true,
            }
        );
        assert_eq!(a[2], Action::Cursor { row: 10, col: 3 });
        assert_eq!(a[3], Action::Key(Key::F(3)));
        assert_eq!(parse("").unwrap(), vec![Action::Key(Key::Enter)]);
        assert!(parse("x=1").is_err());
    }
}
