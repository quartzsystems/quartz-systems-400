// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSVCJOB`: the service console's job, and the runtime every job
//! role shares. Lines come from `slic`; each runs through the command
//! analyzer (`command`): the processing program runs here, in the
//! storage service or as a called program, and whatever the line
//! produced goes back with the prompt to show next. `svc.` lines reach
//! the service's own console table; in SQL mode every line does.

mod debug;
pub mod host;
pub mod jobcmd;
mod log;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use ipc::chan::Chan;
use mi::Job;
use stg::remote::status;
use stg::remote::{RemoteCmds, RemoteProvider, Sources};
use stg::{Client, Shared};
use wm::ctl::{FromJob, PROMPT, Start, ToJob};
use wm::job::Status;

pub use host::Host;

/// The console job's state.
pub struct Console {
    /// The job's surroundings.
    pub host: Host,
    job: Option<Box<Job>>,
    provider: Option<Rc<RefCell<RemoteProvider>>>,
    /// The prompt the storage service last asked for.
    prompt: String,
    /// SRC codes and system values the line's commands raised.
    srcs: Vec<String>,
    sysvals: Option<Vec<u8>>,
}

/// Serve lines until told to end. Never returns.
pub fn serve(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut c = Console::new(ctl, link, start);
    // The console job is the operator's: it never waits on QSYSOPR.
    let dft = vec![("INQMSGRPY".to_string(), "*DFT".to_string())];
    let _ = c
        .host
        .link
        .borrow_mut()
        .with(|l| l.chgjob(String::new(), dft));
    c.started();
    loop {
        status(&c.host.link, Status::Evtw, "");
        let next = loop {
            match c.host.inbox.recv_timeout(host::BREAK_POLL) {
                Ok(m) => break Ok(m),
                Err(RecvTimeoutError::Timeout) => {
                    c.breaks();
                    c.host
                        .children
                        .borrow_mut()
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
                status(&c.host.link, Status::Run, &function);
                if c.prompt == PROMPT && !text.trim().is_empty() {
                    c.job().log_request(&text, ("QSVCJOB", "QSYS"));
                }
                let out = c.line(&text);
                if !out.is_empty() {
                    c.host.output(&out);
                }
                let done = FromJob::Done {
                    prompt: c.prompt.clone(),
                    srcs: std::mem::take(&mut c.srcs),
                    sysvals: c.sysvals.take(),
                };
                if c.host.ctl.borrow_mut().send(&done).is_err() {
                    std::process::exit(0);
                }
                if c.job.as_ref().is_some_and(|j| j.joblog_full) {
                    c.full();
                }
                c.breaks();
            }
            Ok(ToJob::End { .. }) => {
                c.ended(0);
                let _ = c.host.ctl.borrow_mut().send(&FromJob::Ending {
                    reason: "ended by slic".into(),
                });
                std::process::exit(0);
            }
            Ok(_) => {}
            Err(()) => std::process::exit(0),
        }
    }
}

impl Console {
    /// A job's runtime over `link`, its parent on `ctl`.
    pub fn new(ctl: Chan, link: &Shared, start: &Start) -> Self {
        let (inbox_tx, inbox) = inbox(&ctl);
        let display = if start.device.is_empty() || start.role != "INTER" {
            None
        } else {
            match crate::display::Display::connect(
                &start.device,
                &inbox_tx,
                tn5250::ds::wdsf::Model::M3179,
            ) {
                Ok(d) => Some(d),
                Err(e) => {
                    eprintln!("qsjob: {}: {e}", start.name);
                    None
                }
            }
        };
        let lda = stg::remote::new_lda();
        if !start.lda.is_empty() {
            let mut b = start.lda.clone();
            b.resize(stg::remote::LDA_LEN, 0x40);
            *lda.borrow_mut() = b;
        }
        let host = Host {
            link: Rc::clone(link),
            ctl: Rc::new(RefCell::new(ctl)),
            inbox,
            children: Rc::new(RefCell::new(Vec::new())),
            cmds: Rc::new(RefCell::new(RemoteCmds::new(Rc::clone(link)))),
            lda,
            start: Rc::new(start.clone()),
            attrs: Rc::new(RefCell::new(None)),
            display: Rc::new(RefCell::new(display)),
        };
        Self {
            host,
            job: None,
            provider: None,
            prompt: PROMPT.to_string(),
            srcs: Vec::new(),
            sysvals: None,
        }
    }

    /// Show a panel on the job's display.
    pub fn show(&mut self, panel: wm::ctl::Panel) {
        self.host.show(panel);
    }

    /// Wait for the display to answer; `None` when the job is to end.
    pub fn wait_input(&mut self) -> Option<(u8, Vec<String>)> {
        let host = self.host.clone();
        host.wait_input(self.job())
    }

    /// Wait up to `d`, taking breaks; `false` when the job is to end.
    pub fn idle(&mut self, d: Duration) -> bool {
        let host = self.host.clone();
        host.idle(self.job(), d)
    }

    /// Deliver the breaks waiting for the job.
    pub(crate) fn breaks(&mut self) {
        let host = self.host.clone();
        host.breaks(self.job());
    }

    /// One line; its output.
    pub fn line(&mut self, text: &str) -> String {
        let out = self.run_line(text);
        self.sync_log();
        out
    }

    /// Run a command line in a mode (prompted first, checked only);
    /// everything the run produced.
    pub fn run(&mut self, text: &str, mode: crate::command::RunMode) -> crate::command::Outcome {
        let host = self.host.clone();
        let setting = cmd::analyze::Setting {
            interactive: host.interactive(),
            ..cmd::analyze::Setting::default()
        };
        let o = crate::command::run_with(self.job(), &host, text, mode, setting);
        if let Some(p) = &o.prompt {
            self.prompt.clone_from(p);
        }
        self.srcs.extend(o.srcs.iter().cloned());
        if o.sysvals.is_some() {
            self.sysvals.clone_from(&o.sysvals);
        }
        self.after(&o.name);
        self.sync_log();
        o
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
        self.host
            .link
            .borrow_mut()
            .with(|l| stg::Client::joblog_put(l, entries));
    }

    fn run_line(&mut self, text: &str) -> String {
        if self.prompt != PROMPT {
            // SQL mode: the service reads the line as it stands.
            return self.exec(text);
        }
        let host = self.host.clone();
        let o = crate::command::run(self.job(), &host, text);
        if let Some(p) = o.prompt {
            self.prompt = p;
        }
        self.srcs.extend(o.srcs);
        if o.sysvals.is_some() {
            self.sysvals = o.sysvals;
        }
        self.after(&o.name);
        o.text
    }

    /// What a command changes in the job's own state.
    fn after(&mut self, name: &str) {
        match name {
            "CHGLIBL" | "ADDLIBLE" | "RMVLIBLE" | "CHGCURLIB" | "CHGSYSLIBL" => {
                self.host.attrs_changed();
                self.refresh_libl();
            }
            "CHGJOB" => self.host.attrs_changed(),
            "UPDPGM" | "CRTPGM" | "CRTSRVPGM" | "CRTBNDCL" | "CRTCLPGM" | "DLTPGM" => {
                // A changed program is loaded afresh at its next call.
                if let Some(j) = self.job.as_mut() {
                    j.act.loaded.clear();
                }
            }
            "CRTCMD" | "CHGCMD" | "DLTCMD" | "CHGCMDDFT" | "CRTPRXCMD" | "CHGPRXCMD"
            | "DLTPRXCMD" => {
                self.host.cmds.borrow_mut().clear();
            }
            _ => {}
        }
    }

    /// Run a line in the storage service's console table.
    fn exec(&mut self, text: &str) -> String {
        let reply = self.host.exec(text);
        self.prompt = reply.prompt;
        self.srcs.extend(reply.srcs);
        if reply.sysvals.is_some() {
            self.sysvals = reply.sysvals;
        }
        reply.text
    }

    /// The job, created on first use with its sources over the link.
    pub fn job(&mut self) -> &mut Job {
        if self.job.is_none() {
            let link = &self.host.link;
            let mut s = Sources::new(link);
            // The console's `*LDA` is the job's, whether or not a
            // program has run yet.
            s.lda = self.host.lda.clone();
            s.data = stg::remote::RemoteData::new(link.clone(), self.host.lda.clone());
            s.provider = RemoteProvider::new(link.clone(), self.host.lda.clone());
            let mut job = Job::boxed(Box::new(s.objects));
            job.usrspcs.set_source(Box::new(s.spaces));
            job.msgs = Box::new(s.msgs);
            job.ident.name.clone_from(&self.host.start.name);
            job.ident.user.clone_from(&self.host.start.user);
            job.ident.number = format!("{:06}", self.host.start.number);
            job.data = Some(Box::new(s.data));
            job.io = Some(Box::new(s.io));
            // Programs run on this process's main thread; its guard page
            // (`platform::stack`) catches a runaway recursion, so the
            // runtime's own budget never fires first.
            job.stack_bytes = usize::MAX;
            // A running program takes its break messages too.
            let breaks_host = self.host.clone();
            job.attn = Some(Rc::new(move |job: &mut Job| breaks_host.breaks(job)));
            host::attention_timer();
            job.spool = Some(Box::new(stg::remote::spool::RemoteSpool::new(link.clone())));
            let ask = Rc::clone(link);
            job.inquiry = Some(Box::new(move |q| stg::remote::inquiry::answer(&ask, q)));
            let provider = Rc::new(RefCell::new(s.provider));
            let dyn_provider: Rc<RefCell<dyn sysapi::Provider>> = provider.clone();
            sysapi::install(&mut job, dyn_provider);
            // Domain and state are enforced at the level of this IPL.
            job.act.qsecurity = link.borrow_mut().with(Client::whoami).qsecurity;
            let ctl = self.host.ctl.clone();
            let inbox = Rc::clone(&self.host.inbox);
            job.set_debug_hook(Box::new(move |job: &mut Job, stop| {
                debug::session(job, stop, &mut ctl.borrow_mut(), &inbox)
            }));
            crate::command::install_hook(&mut job, &self.host);
            crate::command::natives::install(&mut job, &self.host);
            self.provider = Some(provider);
            self.job = Some(job);
            self.refresh_libl();
        }
        self.job.as_mut().expect("created")
    }

    /// The job's library list, as the service keeps it.
    fn refresh_libl(&mut self) {
        let a = self.host.attrs();
        let Some(j) = self.job.as_mut() else {
            return;
        };
        j.ident.libl = a.libl.search();
        j.ident.curlib = a.libl.cur;
    }
}

/// What `slic` sends, read on a thread.
pub type Inbox = Rc<Receiver<ToJob>>;

/// Read `ctl` on a thread; the channel closes when `slic` does. The
/// sender feeds the job's own display device too.
fn inbox(ctl: &Chan) -> (std::sync::mpsc::Sender<ToJob>, Inbox) {
    let (tx, rx) = std::sync::mpsc::channel();
    let feed = tx.clone();
    if let Ok(mut reader) = ctl.try_clone() {
        std::thread::spawn(move || {
            while let Ok(m) = reader.recv::<ToJob>() {
                if m == (ToJob::End { immediate: true }) {
                    // ENDJOB *IMMED: the service releases what the job held.
                    std::process::exit(2);
                }
                if feed.send(m).is_err() {
                    return;
                }
            }
        });
    }
    (tx, Rc::new(rx))
}
