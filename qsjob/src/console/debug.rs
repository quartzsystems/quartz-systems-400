// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debug session at a stop: commands from the console until one
//! resumes the program.

use ipc::chan::Chan;
use mi::Job;
use mi::debug::{Resume, Stop};
use wm::ctl::{FromJob, ToJob};

/// Serve the stop; how to go on.
pub fn session(job: &mut Job, stop: &Stop, ctl: &mut Chan) -> Resume {
    say(
        ctl,
        &format!(
            "Stopped at {} {} statement {}\n",
            stop.program, stop.procedure, stop.stmt
        ),
    );
    loop {
        say(ctl, "dbg> ");
        let line = match ctl.recv::<ToJob>() {
            Ok(ToJob::Line { text }) => text,
            Ok(ToJob::End { .. }) | Err(_) => std::process::exit(0),
            Ok(ToJob::Start { .. }) => continue,
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [] => {}
            ["step"] | ["step", "over"] => return Resume::StepOver,
            ["step", "into"] => return Resume::StepInto,
            ["go" | "resume" | "g"] => return Resume::Go,
            ["eval", expr] => match job.eval(expr) {
                Ok(v) => say(ctl, &format!("{expr} = {v}\n")),
                Err(e) => say(ctl, &format!("{e}\n")),
            },
            ["dspvar"] => {
                for (n, t) in job.dspvar() {
                    say(ctl, &format!("  {n:<12} {t}\n"));
                }
            }
            ["dspcallstk"] => say(ctl, &job.call_stack_text()),
            ["addbkp", n] => match n.parse::<u32>() {
                Ok(n) => {
                    job.addbkp(&stop.program, n);
                    say(
                        ctl,
                        &format!("Breakpoint at {} statement {n}.\n", stop.program),
                    );
                }
                Err(_) => say(ctl, "addbkp N\n"),
            },
            ["rmvbkp", n] => {
                let gone = match n.parse::<u32>() {
                    Ok(n) => job.rmvbkp(&stop.program, Some(n)),
                    Err(_) => job.rmvbkp(&stop.program, None),
                };
                say(ctl, &format!("{gone} breakpoint(s) removed.\n"));
            }
            ["dmp"] => say(ctl, &job.dump(true)),
            ["help"] => say(
                ctl,
                "step [over|into], go, eval VAR[:x|:c], dspvar, dspcallstk, addbkp N, rmvbkp N|*ALL, dmp\n",
            ),
            _ => say(ctl, "Not a debug command. Type help.\n"),
        }
    }
}

/// Text for the console now.
fn say(ctl: &mut Chan, s: &str) {
    let _ = ctl.send(&FromJob::Output {
        text: s.to_string(),
    });
}
