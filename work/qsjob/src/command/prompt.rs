// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Prompting a command on the job's display: the prompter's screens go
//! out as panels, the keys and fields come back, until the session
//! has a command string or was cancelled.

use cmd::analyze::{Env, Setting};
use cmd::prompt::{Prompter, Screen, Step};
use cmd::{Resolved, analyze};
use mi::Job;
use wm::ctl::Panel;

use super::Fail;
use crate::console::host::Host;

/// Prompt `r`; the command as changed, `None` when cancelled.
pub fn session(job: &mut Job, host: &Host, r: &Resolved) -> Result<Option<Resolved>, Fail> {
    let mut p = Prompter::new(r);
    loop {
        let screen = p.screen();
        host.show(panel(&screen));
        let Some((key, fields)) = host.wait_input(job) else {
            return Ok(None);
        };
        let step = {
            let mut cmds = host.cmds.borrow_mut();
            let attrs = host.attrs();
            let mut env = Env::new(&mut *cmds);
            env.setting = Setting {
                interactive: host.interactive(),
                ..Setting::default()
            };
            env.limited = host.limited();
            env.opts.decfmt = attrs.decfmt.chars().next().unwrap_or(' ');
            env.datfmt.clone_from(&attrs.datfmt);
            p.key(key, &fields, &mut env)
        };
        match step {
            Step::Show => {}
            Step::Cancel => return Ok(None),
            Step::Done(text) => {
                let mut cmds = host.cmds.borrow_mut();
                let mut env = Env::new(&mut *cmds);
                env.setting = Setting {
                    interactive: host.interactive(),
                    ..Setting::default()
                };
                env.limited = host.limited();
                return match analyze(&mut env, &text) {
                    Ok(mut done) => {
                        done.prompt = false;
                        for b in &mut done.parms {
                            b.sel = None;
                        }
                        Ok(Some(done))
                    }
                    Err(e) => Err(Fail {
                        msgid: e.escape.msgid.clone(),
                        data: e.escape.data.clone(),
                        text: e.to_string(),
                    }),
                };
            }
        }
    }
}

/// A prompter screen as a panel.
#[must_use]
pub fn panel(s: &Screen) -> Panel {
    let mut p = Panel::default();
    for (i, row) in s.rows.iter().enumerate() {
        let text = row.trim_end();
        if text.is_empty() {
            continue;
        }
        let attr = if i == 0 {
            1
        } else if i >= 22 {
            2
        } else {
            0
        };
        p.text(u8::try_from(i + 1).unwrap_or(1), 1, attr, text.to_string());
    }
    for f in &s.fields {
        p.field(f.row, f.col, f.len, f.value.clone());
        if f.reverse {
            p.reverse.push((f.row, f.col));
        }
    }
    p.cursor = s.cursor;
    if !s.message.is_empty() {
        p.message.clone_from(&s.message);
        p.error = true;
    }
    p
}
