// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSVCJOB`: the service console's job. Lines come from `slic`; calls,
//! the job's displays and the debugger run here, every other command in
//! the storage service, and whatever the line produced goes back with
//! the prompt to show next.

mod call;
mod debug;
mod jobcmd;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use ipc::chan::Chan;
use mi::Job;
use stg::remote::{RemoteProvider, Sources};
use stg::{Client, Shared};
use stg::remote::status;
use wm::ctl::{FromJob, JOB_COMMANDS, PROMPT, Start, ToJob};
use wm::job::Status;

/// The console job's state.
pub struct Console {
    ctl: Chan,
    link: Shared,
    job: Option<Box<Job>>,
    provider: Option<Rc<RefCell<RemoteProvider>>>,
    /// The prompt the storage service last asked for.
    prompt: String,
    /// SRC codes and system values the line's commands raised.
    srcs: Vec<String>,
    sysvals: Option<Vec<u8>>,
    /// Who the job is.
    start: Start,
}

/// Serve lines until told to end. Never returns.
pub fn serve(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut c = Console {
        start: start.clone(),
        ctl,
        link: Rc::clone(link),
        job: None,
        provider: None,
        prompt: PROMPT.to_string(),
        srcs: Vec::new(),
        sysvals: None,
    };
    loop {
        status(&c.link, Status::Evtw, "");
        match c.ctl.recv::<ToJob>() {
            Ok(ToJob::Line { text }) => {
                let word = text.split_whitespace().next().unwrap_or("");
                let function = if word.is_empty() {
                    String::new()
                } else {
                    format!("CMD-{}", word.to_ascii_uppercase())
                };
                status(&c.link, Status::Run, &function);
                let out = c.line(&text);
                if !out.is_empty() {
                    c.output(&out);
                }
                let done = FromJob::Done {
                    prompt: c.prompt.clone(),
                    srcs: std::mem::take(&mut c.srcs),
                    sysvals: c.sysvals.take(),
                };
                if c.ctl.send(&done).is_err() {
                    std::process::exit(0);
                }
            }
            Ok(ToJob::End { .. }) => {
                let _ = c.ctl.send(&FromJob::Ending {
                    reason: "ended by slic".into(),
                });
                std::process::exit(0);
            }
            Ok(ToJob::Start { .. }) => {}
            Err(_) => std::process::exit(0),
        }
    }
}

impl Console {
    /// Send text to the console now.
    fn output(&mut self, text: &str) {
        let _ = self.ctl.send(&FromJob::Output {
            text: text.to_string(),
        });
    }

    /// One line; its output.
    fn line(&mut self, text: &str) -> String {
        let word = text.split_whitespace().next().unwrap_or("");
        if self.prompt == PROMPT && JOB_COMMANDS.contains(&word) {
            return jobcmd::command(self, word, text);
        }
        self.exec(text)
    }

    /// Run a line in the storage service.
    fn exec(&mut self, text: &str) -> String {
        let reply = self.link.borrow_mut().with(|l| l.exec(text.to_string()));
        self.prompt = reply.prompt;
        self.srcs.extend(reply.srcs);
        if reply.sysvals.is_some() {
            self.sysvals = reply.sysvals;
        }
        let word = text.split_whitespace().next().unwrap_or("");
        if matches!(word, "updpgm" | "crtpgm" | "crtsrvpgm")
            && let Some(j) = self.job.as_mut()
        {
            // A changed program is loaded afresh at its next call.
            j.act.loaded.clear();
        }
        reply.text
    }

    /// The job, created on first use with its sources over the link.
    fn job(&mut self) -> &mut Job {
        if self.job.is_none() {
            let s = Sources::new(&self.link);
            let mut job = Job::boxed(Box::new(s.objects));
            job.usrspcs.set_source(Box::new(s.spaces));
            job.msgs = Box::new(s.msgs);
            job.ident.name.clone_from(&self.start.name);
            job.ident.user.clone_from(&self.start.user);
            job.ident.number = format!("{:06}", self.start.number);
            job.data = Some(Box::new(s.data));
            job.io = Some(Box::new(s.io));
            let provider = Rc::new(RefCell::new(s.provider));
            let dyn_provider: Rc<RefCell<dyn sysapi::Provider>> = provider.clone();
            sysapi::install(&mut job, dyn_provider);
            if let Ok(ctl) = self.ctl.try_clone() {
                let ctl = RefCell::new(ctl);
                job.set_debug_hook(Box::new(move |job: &mut Job, stop| {
                    debug::session(job, stop, &mut ctl.borrow_mut())
                }));
            }
            self.provider = Some(provider);
            self.job = Some(job);
        }
        self.job.as_mut().expect("created")
    }

    /// Commands `QCMDEXC` queued during a call, run now; their output.
    fn pending(&mut self) -> String {
        let lines = self
            .provider
            .as_ref()
            .map(|p| std::mem::take(&mut p.borrow_mut().pending))
            .unwrap_or_default();
        let mut out = String::new();
        for l in lines {
            let _ = writeln!(out, "QCMDEXC: {l}");
            out.push_str(&self.line(&l));
        }
        out
    }
}
