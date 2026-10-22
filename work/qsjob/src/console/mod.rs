// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSVCJOB`: the service console's job. Lines come from `slic`; calls,
//! the job's displays and the debugger run here, every other command in
//! the storage service, and whatever the line produced goes back with
//! the prompt to show next.

mod breaks;
mod call;
mod debug;
mod jobcmd;
mod log;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use ipc::chan::Chan;
use mi::Job;
use stg::remote::status;
use stg::remote::{RemoteProvider, Sources};
use stg::{Client, Shared};
use wm::ctl::{FromJob, JOB_COMMANDS, PROMPT, Start, ToJob};
use wm::job::Status;

/// The console job's state.
pub struct Console {
    ctl: Chan,
    /// Messages from `slic`, read by a thread so the job can wait
    /// with a timeout and look for breaks meanwhile.
    inbox: Inbox,
    pub(crate) link: Shared,
    job: Option<Box<Job>>,
    provider: Option<Rc<RefCell<RemoteProvider>>>,
    /// The job's `*LDA`.
    pub(crate) lda: stg::remote::Lda,
    /// The prompt the storage service last asked for.
    prompt: String,
    /// SRC codes and system values the line's commands raised.
    srcs: Vec<String>,
    sysvals: Option<Vec<u8>>,
    /// Who the job is.
    start: Start,
    /// Jobs this one started (subsystem monitors), until they end.
    pub children: Vec<crate::work::spawn::Spawned>,
}

/// Serve lines until told to end. Never returns.
pub fn serve(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut c = Console::new(ctl, link, start);
    // The console job is the operator's: it never waits on QSYSOPR.
    let dft = vec![("INQMSGRPY".to_string(), "*DFT".to_string())];
    let _ = c.link.borrow_mut().with(|l| l.chgjob(String::new(), dft));
    c.started();
    loop {
        status(&c.link, Status::Evtw, "");
        let next = loop {
            match c.inbox.recv_timeout(BREAK_POLL) {
                Ok(m) => break Ok(m),
                Err(RecvTimeoutError::Timeout) => {
                    c.breaks();
                    c.children
                        .retain_mut(|s| matches!(s.child.try_wait(), Ok(None)));
                }
                Err(RecvTimeoutError::Disconnected) => break Err(()),
            }
        };
        match next {
            Ok(ToJob::Line { text }) => {
                let word = text.split_whitespace().next().unwrap_or("");
                let function = if word.is_empty() {
                    String::new()
                } else {
                    format!("CMD-{}", word.to_ascii_uppercase())
                };
                status(&c.link, Status::Run, &function);
                if c.prompt == PROMPT && !text.trim().is_empty() {
                    c.job().log_request(&text, ("QSVCJOB", "QSYS"));
                }
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
                if c.job.as_ref().is_some_and(|j| j.joblog_full) {
                    c.full();
                }
                c.breaks();
            }
            Ok(ToJob::End { .. }) => {
                c.ended(0);
                let _ = c.ctl.send(&FromJob::Ending {
                    reason: "ended by slic".into(),
                });
                std::process::exit(0);
            }
            Ok(ToJob::Start { .. } | ToJob::Input { .. }) => {}
            Err(()) => std::process::exit(0),
        }
    }
}

impl Console {
    /// Show a panel on the job's display.
    pub fn show(&mut self, panel: wm::ctl::Panel) {
        let _ = self.ctl.send(&FromJob::Show { panel });
    }

    /// Wait for the display to answer: the key (0 for Enter) and the
    /// fields. Breaks are taken meanwhile; `None` when the job is to
    /// end.
    pub fn wait_input(&mut self) -> Option<(u8, Vec<String>)> {
        loop {
            match self.inbox.recv_timeout(BREAK_POLL) {
                Ok(ToJob::Input { key, fields }) => return Some((key, fields)),
                Ok(ToJob::End { .. }) | Err(RecvTimeoutError::Disconnected) => return None,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => self.breaks(),
            }
        }
    }

    /// Wait up to `d`, taking breaks; `false` when the job is to end.
    pub fn idle(&mut self, d: Duration) -> bool {
        match self.inbox.recv_timeout(d) {
            Ok(ToJob::End { .. }) | Err(RecvTimeoutError::Disconnected) => false,
            _ => {
                self.breaks();
                true
            }
        }
    }

    /// A job's runtime over `link`, its parent on `ctl`.
    pub fn new(ctl: Chan, link: &Shared, start: &Start) -> Self {
        let inbox = inbox(&ctl);
        let lda = stg::remote::new_lda();
        if !start.lda.is_empty() {
            let mut b = start.lda.clone();
            b.resize(stg::remote::LDA_LEN, 0x40);
            *lda.borrow_mut() = b;
        }
        Self {
            start: start.clone(),
            inbox,
            ctl,
            link: Rc::clone(link),
            job: None,
            provider: None,
            lda,
            prompt: PROMPT.to_string(),
            srcs: Vec::new(),
            sysvals: None,
            children: Vec::new(),
        }
    }

    /// Send text to the console now.
    fn output(&mut self, text: &str) {
        let _ = self.ctl.send(&FromJob::Output {
            text: text.to_string(),
        });
    }

    /// One line; its output.
    pub fn line(&mut self, text: &str) -> String {
        let out = self.run_line(text);
        self.sync_log();
        out
    }

    /// Bring the storage service's copy of the job log up to date; it is
    /// the copy that is spooled when the job ends, however it ends.
    pub fn sync_log(&mut self) {
        let Some(j) = self.job.as_mut() else {
            return;
        };
        if j.log_outbox.is_empty() {
            return;
        }
        let entries = std::mem::take(&mut j.log_outbox);
        self.link
            .borrow_mut()
            .with(|l| stg::Client::joblog_put(l, entries));
    }

    fn run_line(&mut self, text: &str) -> String {
        let word = text.split_whitespace().next().unwrap_or("");
        if self.prompt == PROMPT && JOB_COMMANDS.contains(&word) {
            return jobcmd::command(self, word, text);
        }
        if self.prompt == PROMPT && word == "dtaara" {
            let args: Vec<&str> = text.split_whitespace().collect();
            if args.get(2).is_some_and(|a| a.eq_ignore_ascii_case("*LDA")) {
                return jobcmd::lda(self, &args[1..]);
            }
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
        if matches!(
            word,
            "chglibl" | "addlible" | "rmvlible" | "chgcurlib" | "chgsyslibl" | "curlib"
        ) {
            self.refresh_libl();
        }
        if matches!(word, "updpgm" | "crtpgm" | "crtsrvpgm")
            && let Some(j) = self.job.as_mut()
        {
            // A changed program is loaded afresh at its next call.
            j.act.loaded.clear();
        }
        reply.text
    }

    /// The job, created on first use with its sources over the link.
    pub fn job(&mut self) -> &mut Job {
        if self.job.is_none() {
            let mut s = Sources::new(&self.link);
            // The console's `*LDA` is the job's, whether or not a
            // program has run yet.
            s.lda = self.lda.clone();
            s.data = stg::remote::RemoteData::new(self.link.clone(), self.lda.clone());
            s.provider = RemoteProvider::new(self.link.clone(), self.lda.clone());
            let mut job = Job::boxed(Box::new(s.objects));
            job.usrspcs.set_source(Box::new(s.spaces));
            job.msgs = Box::new(s.msgs);
            job.ident.name.clone_from(&self.start.name);
            job.ident.user.clone_from(&self.start.user);
            job.ident.number = format!("{:06}", self.start.number);
            job.data = Some(Box::new(s.data));
            job.io = Some(Box::new(s.io));
            // Programs run on this process's main thread (8 MiB): leave
            // room below the budget for the runtime and the handlers.
            job.stack_bytes = 6 << 20;
            job.spool = Some(Box::new(stg::remote::spool::RemoteSpool::new(
                self.link.clone(),
            )));
            let ask = Rc::clone(&self.link);
            job.inquiry = Some(Box::new(move |q| stg::remote::inquiry::answer(&ask, q)));
            let provider = Rc::new(RefCell::new(s.provider));
            let dyn_provider: Rc<RefCell<dyn sysapi::Provider>> = provider.clone();
            sysapi::install(&mut job, dyn_provider);
            // Domain and state are enforced at the level of this IPL.
            job.act.qsecurity = self.link.borrow_mut().with(Client::whoami).qsecurity;
            if let Ok(ctl) = self.ctl.try_clone() {
                let ctl = RefCell::new(ctl);
                let inbox = Rc::clone(&self.inbox);
                job.set_debug_hook(Box::new(move |job: &mut Job, stop| {
                    debug::session(job, stop, &mut ctl.borrow_mut(), &inbox)
                }));
            }
            self.provider = Some(provider);
            self.job = Some(job);
            self.refresh_libl();
        }
        self.job.as_mut().expect("created")
    }

    /// The job's library list, as the service keeps it.
    fn refresh_libl(&mut self) {
        let Some(j) = self.job.as_mut() else {
            return;
        };
        let a = self.link.borrow_mut().with(Client::rtvjoba);
        j.ident.libl = a.libl.search();
        j.ident.curlib = a.libl.cur;
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

/// What `slic` sends, read on a thread.
pub type Inbox = Rc<Receiver<ToJob>>;

/// How often an idle job looks for break messages.
const BREAK_POLL: Duration = Duration::from_secs(1);

/// Read `ctl` on a thread; the channel closes when `slic` does.
fn inbox(ctl: &Chan) -> Inbox {
    let (tx, rx) = std::sync::mpsc::channel();
    if let Ok(mut reader) = ctl.try_clone() {
        std::thread::spawn(move || {
            while let Ok(m) = reader.recv::<ToJob>() {
                if m == (ToJob::End { immediate: true }) {
                    // ENDJOB *IMMED: the service releases what the job held.
                    std::process::exit(2);
                }
                if tx.send(m).is_err() {
                    return;
                }
            }
        });
    }
    Rc::new(rx)
}
