// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Entry points for the layers that start shells:
//!
//! - [`Invocation`] parses `qsh` arguments; [`run`] runs `qsh -c`, a
//!   script file or standard input to completion.
//! - [`Interactive`] is the line-at-a-time session the 5250 QSH
//!   terminal and SSH feed.
//! - [`batch`] is the `QSH CMD()` path: output routing by
//!   `QIBM_QSH_CMD_OUTPUT` and the completion or escape message by
//!   `QIBM_QSH_CMD_ESCAPE_MSG`.

pub mod batch;
mod interactive;
mod invocation;

use std::sync::Arc;

pub use interactive::{Feed, Interactive};
pub use invocation::Invocation;

use crate::msg::Msg;
use crate::shell::{Flow, Shell};
use crate::sys::{Fd, FileType, Sys, stream::read_to_end};

/// Run one startup file if it exists.
fn source_if_exists(sh: &mut Shell, path: &str) {
    let abs = crate::pattern::path::absolute(sh.cwd(), path);
    if !matches!(sh.sys().stat(&abs), Ok(st) if st.ftype == FileType::File) {
        return;
    }
    let Ok(text) = sh.read_file(&abs) else { return };
    let r = sh.run_text(&text, 1);
    if let Err(Flow::Exit(n)) = r {
        sh.status = n;
    }
}

/// Run the startup files: `/etc/profile` and `$HOME/.profile` for a
/// login shell, then the file `$ENV` names for an interactive one.
pub fn startup(sh: &mut Shell, login: bool, interactive: bool) {
    if login {
        source_if_exists(sh, "/etc/profile");
        if let Some(home) = sh.var("HOME") {
            source_if_exists(sh, &crate::pattern::path::join(&home, ".profile"));
        }
    }
    if interactive && let Some(env) = sh.var("ENV") {
        let path = sh.expand_prompt(&env);
        if !path.is_empty() {
            source_if_exists(sh, &path);
        }
    }
}

/// Build a shell for an invocation: arguments, options, startup files.
pub fn shell_for(
    sys: Arc<dyn Sys>,
    inv: &Invocation,
    stdin: Option<Fd>,
    stdout: Option<Fd>,
    stderr: Option<Fd>,
) -> Shell {
    let mut sh = Shell::new(sys, stdin, stdout, stderr);
    for (o, on) in &inv.options {
        sh.set_option(*o, *on);
    }
    let interactive = inv.is_interactive();
    sh.set_interactive(interactive);
    sh.set_args(&inv.arg0, inv.args.clone());
    startup(&mut sh, inv.login, interactive);
    sh
}

/// Run a non-interactive invocation to completion: `-c`, a script file
/// or standard input. The result is the shell's exit status, after its
/// `EXIT` trap.
pub fn run(
    sys: Arc<dyn Sys>,
    inv: &Invocation,
    stdin: Option<Fd>,
    stdout: Option<Fd>,
    stderr: Option<Fd>,
) -> i32 {
    let mut sh = shell_for(sys, inv, stdin, stdout, stderr);
    let text = if let Some(c) = &inv.command {
        c.clone()
    } else if let Some(path) = &inv.script {
        match sh.read_file(path) {
            Ok(t) => t,
            Err(e) => {
                sh.report(&Msg::Open(path.clone(), e.to_string()));
                return 127;
            }
        }
    } else {
        match sh.fd(0) {
            Some(fd) => {
                String::from_utf8_lossy(&read_to_end(&*fd).unwrap_or_default()).into_owned()
            }
            None => String::new(),
        }
    };
    run_text_to_end(&mut sh, &text)
}

/// Run source text as a non-interactive shell's whole input; the exit
/// status after the `EXIT` trap.
pub fn run_text_to_end(sh: &mut Shell, text: &str) -> i32 {
    let r = sh.run_text(text, 1);
    let st = match r {
        Ok(n) | Err(Flow::Exit(n) | Flow::Error(n) | Flow::Fail(n) | Flow::Return(n)) => n,
        Err(Flow::Break(_) | Flow::Continue(_)) => sh.status(),
    };
    sh.status = st;
    sh.run_exit_trap()
}
