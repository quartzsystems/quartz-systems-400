// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CL source in SEU: a line's command prompted by the command prompter
//! with its label and comment (`Prompter::for_source_line`) and laid
//! back into records; a command checked by the analyzer as a CL
//! program would have it. Command definition source has no prompter
//! of its statements: it is checked by the definition compiler and
//! prompted in free format.

use cmd::analyze::{Env, Setting};
use cmd::prompt::{Prompter, Step};
use seu::Session;
use seu::session::{Item, Target};
use seu::srctype::Family;

use super::Ctx;
use crate::command::prompt::panel;

/// Check one command: CL through the analyzer (as a program's
/// statement), a definition statement through the definition compiler.
pub fn analyze(ctx: &mut Ctx<'_>, command: &str, definition: bool) -> Result<(), String> {
    if definition {
        return cmd::source::statements(command)
            .map(|_| ())
            .map_err(|d| d.first().map_or_else(|| "The statement is not valid.".into(), diag_text));
    }
    // A CL statement the compiler knows that no command defines.
    let verb = command.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
    if matches!(
        verb.as_str(),
        "PGM" | "ENDPGM" | "DO" | "ENDDO" | "ELSE" | "DCL" | "DCLF" | "IF" | "MONMSG" | "GOTO" | "CHGVAR"
            | "DOWHILE" | "DOUNTIL" | "DOFOR" | "LEAVE" | "ITERATE" | "SELECT" | "WHEN" | "OTHERWISE"
            | "ENDSELECT" | "SUBR" | "ENDSUBR" | "CALLSUBR" | "RTNSUBR" | "INCLUDE" | "DCLPRCOPT"
            | "COPYRIGHT" | "RETURN"
    ) {
        return Ok(());
    }
    let mut cmds = ctx.host.cmds.borrow_mut();
    let mut env = Env::new(&mut *cmds);
    env.setting = Setting {
        program: true,
        syntax_only: true,
        ..Setting::default()
    };
    match cmd::analyze(&mut env, command) {
        Ok(_) => Ok(()),
        Err(e) => {
            let d = e.diags.first().unwrap_or(&e.escape);
            Err(diag_text(d))
        }
    }
}

fn diag_text(d: &cmd::Diag) -> String {
    if d.detail.is_empty() {
        format!("{} The statement is not valid.", d.msgid)
    } else {
        format!("{} {}", d.msgid, d.detail)
    }
}

/// Prompt the command a line belongs to; the records it spans are
/// replaced by the prompted command.
#[allow(clippy::many_single_char_names)] // the session, the target, the command
pub fn prompt(ctx: &mut Ctx<'_>, s: &mut Session, t: Target) {
    let st = s.srctype();
    if st.family == Family::Cmd {
        let _ = s.start_prompt(t, Some("**"));
        return;
    }
    let recs: Vec<String> = s.buf.records().iter().map(|r| r.data.clone()).collect();
    let (stmt, at) = match t {
        Target::Rec(id) => {
            let Some(i) = s.buf.index_of(id) else {
                return;
            };
            (seu::cl::statement_at(&recs, i), Some(i))
        }
        Target::Ins(_) => (None, None),
    };
    let (label, command, comment) = stmt
        .as_ref()
        .map(|x| (x.label.clone(), x.command.clone(), x.comment.clone()))
        .unwrap_or_default();
    if command.trim().is_empty() {
        s.message = Some("Type a command on the line, then prompt it.".into());
        return;
    }
    let text = if label.is_empty() {
        format!("?{command}")
    } else {
        format!("{label}: ?{command}")
    };
    let resolved = {
        let mut cmds = ctx.host.cmds.borrow_mut();
        let mut env = Env::new(&mut *cmds);
        env.setting = Setting {
            program: true,
            ..Setting::default()
        };
        cmd::analyze(&mut env, &text)
    };
    let r = match resolved {
        Ok(r) => r,
        Err(e) => {
            s.message = Some(diag_text(e.diags.first().unwrap_or(&e.escape)));
            return;
        }
    };
    let mut p = Prompter::for_source_line(&r, &comment);
    let done = loop {
        ctx.host.show(panel(&p.screen()));
        let Some((key, fields)) = ctx.host.wait_input(ctx.job) else {
            return;
        };
        let step = {
            let mut cmds = ctx.host.cmds.borrow_mut();
            let mut env = Env::new(&mut *cmds);
            env.setting = Setting {
                program: true,
                ..Setting::default()
            };
            p.key(key, &fields, &mut env)
        };
        match step {
            Step::Show => {}
            Step::Cancel => return,
            Step::Done(t) => break t,
        }
    };
    let (label, comment) = p.source_parts().unwrap_or_default();
    let lines = seu::cl::format(&label, &done, &comment, s.buf.width.min(80));
    let Some(stmt) = stmt else {
        return;
    };
    let first = stmt.first;
    let last = stmt.last.max(at.unwrap_or(first));
    s.buf.remove(first, last);
    let after = first.checked_sub(1);
    match s.buf.insert(after, &lines, s.defaults.increment, s.today) {
        Ok(ids) => {
            s.touched.extend(ids.iter().copied());
            if let Some(id) = ids.first() {
                s.reveal(&Item::Rec(*id));
            }
        }
        Err(e) => s.message = Some(format!("{e}: resequence the member first.")),
    }
    super::check(ctx, s);
}
