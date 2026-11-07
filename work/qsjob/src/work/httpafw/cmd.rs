// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::cmd::Cmd`: commands run in this job with the job
//! swapped to the signed-on user (the analyzer Command Entry uses, the
//! user's authority), their messages the job log entries the run added;
//! definitions from the `*CMD` objects the user's library list finds,
//! the choices text the 5250 prompter shows. Recent and saved commands
//! are kept with the user's preferences (`QUSRSYS/QZXWPRF`, keys
//! starting `_c`); a command shared with a group or `*PUBLIC` is copied
//! under that profile (`_sh.<owner>.<n>`).

use std::collections::HashMap;
use std::sync::Mutex;

use accessweb::env::cmd::{Cmd, CmdDef, How, LogMessage, Outcome, Param, Ran, Saved};
use cmd::analyze::CmdSource as _;
use cmd::def::PmtCtlRef;
use qsweb::msg::{Kind, Message};
use stg::Client;

use super::Link;
use crate::command::RunMode;
use crate::sysdsp::keys;
use crate::work::web::bridge::Main;

/// The program the job log names as the requester.
const PGM: (&str, &str) = ("QZXWCMD", "QSYS");
/// The preference store.
const STORE: &str = "QZXWPRF";
/// The most bytes of a value in one store entry.
const CHUNK: usize = 90;
/// Results kept per user.
const KEEP: usize = 20;
/// Escapes that mean the command is not there.
const NOT_FOUND: &[&str] = &["CPD0030", "CPF9801"];

/// Commands.
pub struct LiveCmd {
    link: Link,
    results: Mutex<HashMap<String, Vec<Ran>>>,
    running: Mutex<Option<String>>,
}

impl LiveCmd {
    /// Over the job's thread.
    pub fn new(link: Link) -> Self {
        Self {
            link,
            results: Mutex::default(),
            running: Mutex::default(),
        }
    }

    fn read(&self, who: &str) -> Vec<(String, String)> {
        let w = who.to_string();
        self.link
            .job(move |m| {
                m.ask(|l| l.web_store(STORE.into(), w, None))
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    fn write(&self, who: &str, values: Vec<(String, String)>) -> Result<(), Message> {
        let w = who.to_string();
        let r = self.link.job(move |m| {
            m.ask(|l| l.web_store(STORE.into(), w, Some(values)))
                .map(|_| ())
        });
        r.unwrap_or_else(|| Err("CPF9897".into()))
            .map_err(|id: String| {
                Message::new(
                    Kind::Error,
                    &id,
                    &format!("The Access for Web store QUSRSYS/{STORE} could not be changed."),
                )
            })
    }

    /// Replace `who`'s entries under `prefix` with `values`.
    fn replace(
        &self,
        who: &str,
        prefix: &str,
        values: Vec<(String, String)>,
    ) -> Result<(), Message> {
        let mut all: Vec<(String, String)> = self
            .read(who)
            .into_iter()
            .filter(|(k, _)| !k.starts_with(prefix))
            .collect();
        all.extend(values);
        self.write(who, all)
    }
}

/// `text` in store-sized pieces, cut at character boundaries.
fn chunks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if cur.len() + c.len_utf8() > CHUNK {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// The pieces of `base` (`base.00`, `base.01`, ...) put together.
fn joined(entries: &[(String, String)], base: &str) -> String {
    let mut parts: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(base)
                .and_then(|r| r.strip_prefix('.'))
                .map(|n| (n, v.as_str()))
        })
        .collect();
    parts.sort_unstable();
    parts.into_iter().map(|(_, v)| v).collect()
}

fn pieces(base: &str, text: &str) -> Vec<(String, String)> {
    chunks(text)
        .into_iter()
        .enumerate()
        .map(|(i, c)| (format!("{base}.{i:02}"), c))
        .collect()
}

/// A saved command's entries under `base` (`_cs.03`).
fn saved_entries(base: &str, s: &Saved) -> Vec<(String, String)> {
    let name: String = s.name.chars().take(50).collect();
    let mut v = vec![(
        base.to_string(),
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{name}",
            s.how.code(),
            s.last,
            s.share
        ),
    )];
    v.extend(pieces(base, &s.command));
    v
}

/// The saved commands under `prefix` (`_cs`, `_sh.OWNER`), by number.
fn saved_from(entries: &[(String, String)], prefix: &str) -> Vec<(String, Saved)> {
    let mut out: Vec<(String, Saved)> = entries
        .iter()
        .filter_map(|(k, v)| {
            let n = k.strip_prefix(prefix)?.strip_prefix('.')?;
            if n.contains('.') {
                return None;
            }
            let mut f = v.splitn(4, '\u{1f}');
            let how = How::from_code(f.next().unwrap_or(""));
            let last = f.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let share = f.next().unwrap_or("").to_string();
            let name = f.next().unwrap_or("").to_string();
            Some((
                n.to_string(),
                Saved {
                    name,
                    command: joined(entries, k),
                    how,
                    last,
                    share,
                },
            ))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A command's output text as messages, one a line; a leading
/// message ID (`CPC1221 ...`) is the line's.
fn text_lines(text: &str) -> Vec<LogMessage> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (first, rest) = l.split_once(' ').unwrap_or((l, ""));
            let is_id = first.len() == 7
                && first.chars().take(3).all(|c| c.is_ascii_uppercase())
                && first.chars().skip(3).all(|c| c.is_ascii_hexdigit());
            LogMessage {
                severity: 0,
                id: if is_id {
                    first.to_string()
                } else {
                    String::new()
                },
                text: if is_id {
                    rest.to_string()
                } else {
                    l.to_string()
                },
                program: String::new(),
                sent_ms: now,
            }
        })
        .collect()
}

/// A job log entry as Run command lists it.
fn log_of(e: &wm::joblog::Entry) -> LogMessage {
    LogMessage {
        severity: u8::try_from(e.severity).unwrap_or(99),
        id: e.msgid.trim().to_string(),
        text: e.text.clone(),
        program: e.from.0.trim().to_string(),
        sent_ms: e.time / 1000,
    }
}

/// Run (or check) `command` in the job, as whoever it runs as now.
fn run_here(m: &mut Main, command: &str, check: bool) -> Ran {
    let _ = keys::take_outside_end();
    let _request = keys::Request::enter();
    let before = m.console.job().joblog.len();
    if !check {
        m.console.job().log_request(command, PGM);
    }
    let o = m
        .console
        .run(command, if check { RunMode::Check } else { RunMode::Run });
    let job = {
        let j = m.console.job();
        format!("{}/{}/{}", j.ident.number, j.ident.user, j.ident.name)
    };
    let mut messages: Vec<LogMessage> = m
        .console
        .job()
        .joblog
        .entries()
        .skip(before)
        .filter(|e| e.kind != "Request")
        .map(log_of)
        .collect();
    // A command that answered with text only (SBMJOB's completion): its lines.
    if messages.is_empty() {
        messages = text_lines(&o.text);
    }
    let mut ids: Vec<String> = Vec::new();
    if let Some(f) = &o.escape {
        ids.push(f.msgid.clone());
    }
    if let Some(d) = &o.diag {
        ids.push(d.msgid.clone());
    }
    let outcome = if o.escape.is_none() {
        if check {
            Outcome::Checked
        } else {
            Outcome::Completed
        }
    } else if ids.iter().any(|i| i == keys::ENDED) {
        Outcome::Cancelled
    } else if ids.iter().any(|i| NOT_FOUND.contains(&i.as_str())) {
        Outcome::NotFound
    } else if check {
        Outcome::CheckFailed
    } else {
        Outcome::Failed
    };
    Ran {
        command: command.to_string(),
        job,
        outcome,
        messages,
    }
}

/// The web F4's view of a definition.
fn def_of(d: &cmd::def::CmdDef, lib: String) -> CmdDef {
    let params = d
        .parms
        .iter()
        .filter(|p| p.constant.is_none())
        .map(|p| {
            let mut values = Vec::new();
            if p.rstd {
                for v in p
                    .sngval
                    .iter()
                    .map(|s| &s.from)
                    .chain(&p.values)
                    .chain(p.spcval.iter().map(|s| &s.from))
                {
                    if !values.contains(v) {
                        values.push(v.clone());
                    }
                }
            }
            Param {
                keyword: p.kwd.clone(),
                prompt: p
                    .prompt
                    .as_ref()
                    .map(|(t, _)| t.clone())
                    .unwrap_or_default(),
                choices: cmd::prompt::layout::choices(p),
                values,
                default: p.dft.clone(),
                max: p.max.max(1),
                size: u32::try_from(cmd::prompt::layout::field_width(p).clamp(1, 80)).unwrap_or(10),
                required: p.min > 0,
                additional: matches!(p.pmtctl, Some(PmtCtlRef::PmtRqs | PmtCtlRef::Label { .. })),
            }
        })
        .collect();
    CmdDef {
        name: d.name.clone(),
        lib,
        text: d.prompt.clone(),
        params,
    }
}

impl Cmd for LiveCmd {
    fn run(&self, user: &str, command: &str, check: bool) -> Ran {
        if let Ok(mut r) = self.running.lock() {
            *r = Some(user.to_string());
        }
        let (u, c) = (user.to_string(), command.to_string());
        // BLOCKED(rel): the bridge waits 120 seconds for the job's thread; a longer command
        // goes on running and this run reports CPF9897 (submit long ones in batch).
        let ran = self
            .link
            .bridge
            .call(move |m| m.as_user(&u, |m| run_here(m, &c, check)))
            .unwrap_or_else(|| Err(super::gone()));
        if let Ok(mut r) = self.running.lock() {
            *r = None;
        }
        let ran = ran.unwrap_or_else(|e| Ran {
            command: command.to_string(),
            job: String::new(),
            outcome: Outcome::Failed,
            messages: vec![LogMessage {
                severity: 40,
                id: e.id,
                text: e.text,
                program: String::new(),
                sent_ms: 0,
            }],
        });
        if let Ok(mut all) = self.results.lock() {
            let v = all.entry(user.to_string()).or_default();
            v.push(ran.clone());
            let extra = v.len().saturating_sub(KEEP);
            v.drain(..extra);
        }
        ran
    }

    fn results(&self, user: &str) -> Vec<Ran> {
        self.results
            .lock()
            .ok()
            .and_then(|r| r.get(user).cloned())
            .unwrap_or_default()
    }

    fn clear(&self, user: &str) {
        if let Ok(mut r) = self.results.lock() {
            r.remove(user);
        }
    }

    fn cancel(&self, user: &str) -> bool {
        let mine = self
            .running
            .lock()
            .is_ok_and(|r| r.as_deref() == Some(user));
        if mine {
            keys::end_from_outside();
        }
        mine
    }

    fn job_queues(&self, user: &str) -> Vec<String> {
        self.link.read(user, |m| {
            let mut v: Vec<String> = m
                .ask(|l| l.list_objects("*ALL".into(), "*ALL".into(), "*JOBQ".into()))
                .into_iter()
                .map(|o| format!("{}/{}", o.lib, o.name))
                .collect();
            v.sort();
            // QBATCH first: the queue the shipped job descriptions name.
            if let Some(i) = v.iter().position(|q| q == "QGPL/QBATCH") {
                let q = v.remove(i);
                v.insert(0, q);
            }
            v
        })
    }

    fn describe(&self, user: &str, name: &str) -> Result<CmdDef, Message> {
        let (lib, n) = match name.split_once('/') {
            Some((l, n)) => (
                Some(l.trim().to_ascii_uppercase()),
                n.trim().to_ascii_uppercase(),
            ),
            None => (None, name.trim().to_ascii_uppercase()),
        };
        let shown = lib.clone().unwrap_or_else(|| "*LIBL".into());
        let r = self.link.read(user, move |m| {
            // The command cache, not the link: its borrow ends here.
            let found = std::cell::RefCell::borrow_mut(&m.console.host.cmds)
                .lookup(&n, lib.as_deref().filter(|l| *l != "*LIBL"));
            Some(match found {
                Ok(Some((obj, lib))) => Ok(def_of(&obj.effective(), lib)),
                Ok(None) => Err(("CPD0030".to_string(), n)),
                Err(e) => Err((e.msgid().to_string(), n)),
            })
        });
        match r {
            Some(Ok(d)) => Ok(d),
            Some(Err((id, n))) => Err(Message::new(
                Kind::Error,
                &id,
                &format!("Command {n} in library {shown} not found."),
            )
            .with_data(&[&n, &shown])),
            None => Err(super::gone()),
        }
    }

    fn recent(&self, user: &str) -> Vec<String> {
        let all = self.read(user);
        let mut bases: Vec<&str> = all
            .iter()
            .filter_map(|(k, _)| {
                k.strip_prefix("_cr.")
                    .map(|r| r.split('.').next().unwrap_or(""))
            })
            .collect();
        bases.sort_unstable();
        bases.dedup();
        bases
            .into_iter()
            .map(|b| joined(&all, &format!("_cr.{b}")))
            .filter(|c| !c.is_empty())
            .collect()
    }

    fn put_recent(&self, user: &str, commands: &[String]) -> Result<(), Message> {
        let v = commands
            .iter()
            .take(50)
            .enumerate()
            .flat_map(|(i, c)| pieces(&format!("_cr.{i:02}"), c))
            .collect();
        self.replace(user, "_cr.", v)
    }

    fn saved(&self, user: &str) -> Vec<Saved> {
        saved_from(&self.read(user), "_cs")
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    fn put_saved(&self, user: &str, saved: &[Saved]) -> Result<(), Message> {
        let before = self.saved(user);
        let v = saved
            .iter()
            .take(100)
            .enumerate()
            .flat_map(|(i, s)| saved_entries(&format!("_cs.{i:02}"), s))
            .collect();
        self.replace(user, "_cs.", v)?;
        // The copies under the groups and *PUBLIC it is shared with.
        let mut to: Vec<&str> = before
            .iter()
            .chain(saved)
            .map(|s| s.share.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        to.sort_unstable();
        to.dedup();
        let prefix = format!("_sh.{user}.");
        for t in to {
            let mine = saved
                .iter()
                .enumerate()
                .filter(|(_, s)| s.share == t)
                .flat_map(|(i, s)| saved_entries(&format!("_sh.{user}.{i:02}"), s))
                .collect();
            self.replace(t, &prefix, mine)?;
        }
        Ok(())
    }

    fn shared_with(&self, user: &str, to: &[String]) -> Vec<(String, Saved)> {
        let mut out = Vec::new();
        for t in to {
            let all = self.read(t);
            let mut owners: Vec<&str> = all
                .iter()
                .filter_map(|(k, _)| k.strip_prefix("_sh.").and_then(|r| r.split('.').next()))
                .collect();
            owners.sort_unstable();
            owners.dedup();
            for o in owners.into_iter().filter(|o| *o != user) {
                out.extend(
                    saved_from(&all, &format!("_sh.{o}"))
                        .into_iter()
                        .map(|(_, s)| (o.to_string(), s)),
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_commands() {
        let long = "SBMJOB CMD(CALL PGM(HLINV/INVLOAD) PARM('*ALL' 'a very long parameter value that goes on')) JOB(INVBATCH) JOBQ(HLINV/INVLOAD)";
        let s = Saved {
            name: "Load".into(),
            command: long.into(),
            how: How::Batch,
            last: 42,
            share: "ADMINS".into(),
        };
        let e = saved_entries("_cs.00", &s);
        assert!(
            e.len() > 2
                && e.iter()
                    .all(|(k, v)| k.len() <= 20 && v.len() <= CHUNK + 10)
        );
        assert_eq!(saved_from(&e, "_cs"), [("00".to_string(), s)]);
        assert_eq!(joined(&pieces("_cr.03", long), "_cr.03"), long);
        assert_eq!(chunks(""), [""]);
        let t = text_lines("CPC1221 Job 000057/QSECOFR/QDFTJOBD submitted.\nplain line\n");
        assert_eq!(
            (t[0].id.as_str(), t[0].text.as_str()),
            ("CPC1221", "Job 000057/QSECOFR/QDFTJOBD submitted.")
        );
        assert_eq!((t[1].id.as_str(), t[1].text.as_str()), ("", "plain line"));
    }
}
