// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keystroke script: one step per line, `#` comments. The same
//! script runs against every target, so it names what it waits for
//! rather than what it expects the host to send.
//!
//! ```text
//! wait "Sign On"            until the text shows and the keyboard is free
//! expect "Sign On"          the text shows now (an assertion)
//! absent "CPF"              the text does not show
//! type QSECOFR              type characters (the rest of the line)
//! key enter                 enter, f1..f24, pgup, pgdn, tab, backtab,
//!                           fieldexit, field+, field-, eraseeof,
//!                           eraseinput, home, reset, clear, help, print,
//!                           attn, sysreq, insert, delete, backspace, dup,
//!                           newline, up, down, left, right
//! cursor 6 53               1-based row and column
//! field 6 53 QSECOFR        cursor there, then type
//! signon                    user and password from the target, Enter
//! snap NAME [+mask] [-mask] capture or compare a golden
//! show                      print the screen
//! sleep 500                 milliseconds
//! mask NAME REGEX => TOKEN  a mask for the following snaps
//! echo text
//! continue                  keep going after a failed step
//! ```

use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use tn5250::term::{Key, KeyResult};

use crate::RunArgs;
use crate::env::Credentials;
use crate::golden::{self, Entry};
use crate::mask::{Mask, Masks};
use crate::session::Session;

/// What the scripts share over a session.
pub struct Context {
    /// The masks in force.
    pub masks: Masks,
    /// The golden directory.
    pub golden: Option<std::path::PathBuf>,
    /// Write instead of compare.
    pub update: bool,
    /// The target's name.
    pub source: String,
    /// The release to record.
    pub release: String,
    /// The user and password.
    pub user: String,
    password: String,
    /// The running script's name.
    pub script: String,
    /// Keep going after a failed step.
    pub continue_on_error: bool,
    /// The wait timeout.
    pub timeout: Duration,
    /// Snapshots compared that differed.
    pub mismatches: Vec<String>,
    /// Index entries to append.
    pub entries: Vec<Entry>,
}

impl Context {
    /// From the arguments.
    pub fn new(args: &RunArgs, creds: &Credentials) -> Self {
        let user = args.user.clone().unwrap_or_else(|| creds.user.clone());
        let mut masks = Masks::defaults();
        masks.set_user(&user);
        Self {
            masks,
            golden: args.golden.clone(),
            update: args.update,
            source: args.target.name().into(),
            release: args.release.clone(),
            user,
            password: args
                .password
                .clone()
                .unwrap_or_else(|| creds.password.clone()),
            script: String::new(),
            continue_on_error: false,
            timeout: Duration::from_millis(args.timeout),
            mismatches: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// Write the index entries; fail on mismatches.
    pub fn finish(&self) -> Result<()> {
        if let Some(dir) = &self.golden
            && self.update
            && !self.entries.is_empty()
        {
            golden::index_append(dir, &self.entries)?;
        }
        if !self.mismatches.is_empty() {
            bail!(
                "{} snapshot(s) differ: {}",
                self.mismatches.len(),
                self.mismatches.join(", ")
            );
        }
        Ok(())
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && (s.starts_with('"') && s.ends_with('"') || s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// The key a script names.
pub fn key_named(name: &str) -> Option<Key> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "enter" => Key::Enter,
        "pgup" | "pageup" | "rolldown" => Key::PageUp,
        "pgdn" | "pagedown" | "rollup" => Key::PageDown,
        "tab" => Key::Tab,
        "backtab" | "shift-tab" => Key::BackTab,
        "fieldexit" | "field-exit" => Key::FieldExit,
        "field+" | "fieldplus" => Key::FieldPlus,
        "field-" | "fieldminus" => Key::FieldMinus,
        "eraseeof" | "erase-eof" => Key::EraseEof,
        "eraseinput" | "erase-input" => Key::EraseInput,
        "home" => Key::Home,
        "reset" => Key::Reset,
        "clear" => Key::Clear,
        "help" => Key::Help,
        "print" => Key::Print,
        "attn" => Key::Attn,
        "sysreq" | "sysrq" => Key::SysReq,
        "testreq" => Key::TestReq,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        "backspace" => Key::Backspace,
        "dup" => Key::Dup,
        "fieldmark" => Key::FieldMark,
        "newline" => Key::NewLine,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "recbksp" => Key::RecordBackspace,
        _ => {
            let f = n.strip_prefix('f')?;
            let num: u8 = f.parse().ok()?;
            if (1..=24).contains(&num) {
                Key::F(num)
            } else {
                return None;
            }
        }
    })
}

/// Run one script.
pub fn run(s: &mut Session, ctx: &mut Context, text: &str) -> Result<()> {
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        step(s, ctx, line).with_context(|| format!("line {}: {line}", i + 1))?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // one arm per step
fn step(s: &mut Session, ctx: &mut Context, line: &str) -> Result<()> {
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    match word.to_ascii_lowercase().as_str() {
        "wait" => {
            let want = unquote(rest);
            let w = want.clone();
            if !s.wait_for(ctx.timeout, move |t| t.contains(&w) && !t.is_locked())? {
                bail!("did not see {want:?}");
            }
            s.settle(Duration::from_millis(150))?;
        }
        "unlock" => {
            if !s.wait_unlocked(ctx.timeout)? {
                bail!("keyboard stayed locked");
            }
        }
        "settle" => s.settle(Duration::from_millis(rest.parse().unwrap_or(300)))?,
        "expect" => {
            s.wait_unlocked(ctx.timeout)?;
            let want = unquote(rest);
            if !s.term.contains(&want) {
                bail!("{want:?} is not on the screen");
            }
        }
        "absent" => {
            s.wait_unlocked(ctx.timeout)?;
            let want = unquote(rest);
            if s.term.contains(&want) {
                bail!("{want:?} is on the screen");
            }
        }
        "type" => {
            s.wait_unlocked(ctx.timeout)?;
            check(s.type_str(&unquote(rest))?)?;
        }
        "key" => {
            let k = key_named(rest).ok_or_else(|| anyhow!("unknown key {rest}"))?;
            if !matches!(k, Key::Reset | Key::Attn | Key::SysReq) {
                s.wait_unlocked(ctx.timeout)?;
            }
            check(s.key(k)?)?;
        }
        "cursor" => {
            let mut it = rest.split_whitespace();
            let r: usize = it.next().context("row")?.parse()?;
            let c: usize = it.next().context("column")?.parse()?;
            s.term.set_cursor(r, c);
        }
        "field" => {
            let mut it = rest.splitn(3, char::is_whitespace);
            let r: usize = it.next().context("row")?.parse()?;
            let c: usize = it.next().context("column")?.parse()?;
            let text = unquote(it.next().unwrap_or(""));
            s.wait_unlocked(ctx.timeout)?;
            s.term.set_cursor(r, c);
            check(s.type_str(&text)?)?;
        }
        "signon" => {
            s.wait_unlocked(ctx.timeout)?;
            let (user, pw) = if rest.is_empty() {
                (ctx.user.clone(), ctx.password.clone())
            } else {
                let mut it = rest.split_whitespace();
                (
                    it.next().unwrap_or("").to_string(),
                    it.next().unwrap_or("").to_string(),
                )
            };
            // The user field is the first input field, the password the
            // second, on every sign-on display.
            let (f0, f1) = {
                let f = s.term.fields();
                if f.len() < 2 {
                    bail!("the sign-on display has {} fields", f.len());
                }
                (f[0].start, f[1].start)
            };
            let cols = s.term.cols();
            s.term.set_cursor(f0 / cols + 1, f0 % cols + 1);
            check(s.type_str(&user)?)?;
            s.term.set_cursor(f1 / cols + 1, f1 % cols + 1);
            check(s.type_str(&pw)?)?;
            check(s.key(Key::Enter)?)?;
        }
        "snap" => {
            s.wait_unlocked(ctx.timeout)?;
            let mut it = rest.split_whitespace();
            let name = it.next().context("snapshot name")?.to_string();
            let mut masks = ctx.masks.clone();
            for m in it {
                if let Some(n) = m.strip_prefix('+') {
                    masks.enable(n)?;
                } else if let Some(n) = m.strip_prefix('-') {
                    masks.disable(n);
                }
            }
            let rows = masks.apply(&s.term.text_rows());
            let attrs = s.term.attr_rows();
            match &ctx.golden {
                None => {
                    println!("--- {name} ---");
                    for r in &rows {
                        println!("|{r}|");
                    }
                }
                Some(dir) if ctx.update => {
                    golden::write(dir, &name, &rows, &attrs)?;
                    ctx.entries.push(Entry {
                        name: name.clone(),
                        source: ctx.source.clone(),
                        release: ctx.release.clone(),
                        script: ctx.script.clone(),
                        width: s.term.cols(),
                    });
                    println!("  wrote {name}");
                }
                Some(dir) => {
                    if let Some(diff) = golden::compare(dir, &name, &rows, &attrs)? {
                        println!("  {name} differs:\n{diff}");
                        ctx.mismatches.push(name);
                    } else {
                        println!("  {name} matches");
                    }
                }
            }
        }
        "show" => {
            s.settle(Duration::from_millis(200))?;
            let (r, c) = s.term.cursor();
            println!(
                "--- screen (cursor {r},{c}; {}) ---",
                if s.term.is_locked() {
                    "locked"
                } else {
                    "unlocked"
                }
            );
            for row in s.term.text_rows() {
                println!("|{row}|");
            }
        }
        "sleep" => std::thread::sleep(Duration::from_millis(rest.parse()?)),
        "mask" => {
            let m = Mask::parse(rest)?;
            ctx.masks.add(m);
        }
        "nomask" => ctx.masks.disable(rest),
        "echo" => println!("{rest}"),
        "continue" => ctx.continue_on_error = true,
        "timeout" => ctx.timeout = Duration::from_millis(rest.parse()?),
        other => bail!("unknown step {other}"),
    }
    Ok(())
}

fn check(r: KeyResult) -> Result<()> {
    match r {
        KeyResult::Done | KeyResult::Sent => Ok(()),
        KeyResult::Locked => bail!("the keyboard is locked"),
        KeyResult::Error(code) => bail!("operator error {code:04}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_by_name() {
        assert_eq!(key_named("F12"), Some(Key::F(12)));
        assert_eq!(key_named("pgdn"), Some(Key::PageDown));
        assert_eq!(key_named("f25"), None);
    }

    #[test]
    fn unquotes() {
        assert_eq!(unquote("\"a b\""), "a b");
        assert_eq!(unquote("a b"), "a b");
    }
}
