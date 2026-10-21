// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Everything the IPL and the service console share.

use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use licpart::{Device, IplMode, IplType, LicPart, Record, Slot};
use sysval::SysVals;

use console::Command;

use crate::hwrsrc::Table;
use crate::ipl::cmdline::Boot;
use crate::ipl::log::IplLog;
use crate::ipl::storage::{Storage, Temp};
use crate::ipl::{Event, lic, park};
use crate::net::Link;
use crate::platform::console::Outputs;
use crate::src::Src;

/// Which process the code runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// PID 1: the IPL, the console, power down.
    Slic,
    /// The storage service: the store, the database, the requests of
    /// the jobs. SRC codes and the LIC record stay with `slic`.
    Storage,
}

/// State of this IPL.
pub struct System {
    /// Console and serial writers.
    pub out: Outputs,
    /// Boot parameters.
    pub boot: Boot,
    /// Resolved keylock mode.
    pub mode: IplMode,
    /// Every SRC so far.
    pub log: IplLog,
    /// The LIC partition and its current record, once attached.
    pub lic: Option<(LicPart<Box<dyn Device>>, Record)>,
    /// Unix time this IPL started.
    pub start_time: u64,
    /// The previous IPL was found in progress.
    pub abnormal: bool,
    /// Set once C6001090 has been written.
    pub complete: bool,
    /// Hardware resource table.
    pub hw: Table,
    /// Network links as enumerated at IPL.
    pub links: Vec<Link>,
    /// Unix time the clock was set to from the RTC, if it was.
    pub clock: Option<u64>,
    /// System values, once loaded in the OS phase.
    pub sysvals: Option<SysVals>,
    /// The console thread, once attached.
    pub console: Option<Sender<Command>>,
    /// ASP 1 and its store, once opened.
    pub storage: Option<Storage>,
    /// The temporary ASP with QTEMP, once created.
    pub temp: Option<Temp>,
    /// The service console's database state: modes, xfer buffers, SQL.
    pub svc: crate::svc::db::State,
    /// What this process is: PID 1, or the storage service.
    pub role: Role,
    /// SRC codes the storage service raised, for `slic` to show.
    pub raised: Vec<&'static Src>,
    /// The system values changed since the last request.
    pub sysvals_changed: bool,
    /// Record locks, one table for every job.
    pub locks: db::odp::SharedLocks,
    /// The store as lent to the job sources for one request.
    pub lent: crate::svc::ile::Shared,
    /// Each active job's attributes, by number (storage service).
    pub attrs: std::collections::BTreeMap<u32, wm::job::JobAttrs>,
    /// The job a console command runs for (storage service).
    pub current_job: u32,
    /// Message queues in break delivery, by the job they are allocated
    /// to (storage service).
    pub break_alloc: std::collections::HashMap<store::obj::ObjectId, u32>,
    /// Break messages waiting for each job: queue and message.
    pub break_pending: std::collections::BTreeMap<u32, Vec<crate::stgsvc::msgs::Break>>,
    /// Active subsystems by name (storage service).
    pub sbs: std::collections::BTreeMap<String, crate::stgsvc::sbs::Active>,
    /// Storage pools and activity levels (storage service).
    pub pools: wm::pools::PoolTable,
    /// Jobs waiting on job queues: number and queue (storage service).
    pub queued: std::collections::BTreeMap<u32, String>,
    /// Jobs taken off a queue, until they say hello.
    pub pending: std::collections::BTreeMap<u32, wm::obj::jobq::QJob>,
    /// Which subsystem holds each job queue.
    pub jobq_attach: std::collections::BTreeMap<String, String>,
    /// Jobs `ENDJOB` asked to end, and whether `*IMMED`.
    pub end_requests: std::collections::BTreeMap<u32, bool>,
    /// The active jobs (storage service).
    pub jobs: wm::table::JobTable,
    /// Every spooled file (storage service).
    pub spool: crate::stgsvc::spool::Spool,
    /// The storage service and the service console job, once started.
    pub procs: crate::procs::Procs,
    /// The event channel.
    pub events: Option<Rc<Receiver<Event>>>,
}

impl System {
    /// Fresh state before the LIC is attached.
    pub fn new(out: Outputs, boot: Boot) -> Self {
        let mode = boot.mode.unwrap_or(IplMode::Normal);
        Self {
            out,
            boot,
            mode,
            log: IplLog::new(),
            lic: None,
            start_time: now_unix(),
            abnormal: false,
            complete: false,
            hw: Table::default(),
            links: Vec::new(),
            clock: None,
            sysvals: None,
            console: None,
            storage: None,
            temp: None,
            svc: crate::svc::db::State::default(),
            events: None,
            role: Role::Slic,
            raised: Vec::new(),
            sysvals_changed: false,
            locks: db::odp::new_shared(),
            lent: std::rc::Rc::new(std::cell::RefCell::new(None)),
            procs: crate::procs::Procs::default(),
            jobs: wm::table::JobTable::default(),
            spool: crate::stgsvc::spool::Spool::default(),
            queued: std::collections::BTreeMap::new(),
            pending: std::collections::BTreeMap::new(),
            jobq_attach: std::collections::BTreeMap::new(),
            end_requests: std::collections::BTreeMap::new(),
            sbs: std::collections::BTreeMap::new(),
            pools: wm::pools::PoolTable::default(),
            break_alloc: std::collections::HashMap::new(),
            break_pending: std::collections::BTreeMap::new(),
            attrs: std::collections::BTreeMap::new(),
            current_job: 0,
        }
    }

    /// The IPL type for this boot: D from installation media, else the
    /// slot's letter.
    pub fn ipl_type(&self) -> IplType {
        if self.boot.ipl_d {
            return IplType::D;
        }
        match self.boot.slot {
            Slot::A => IplType::A,
            Slot::B => IplType::B,
        }
    }

    /// Log a code and show it on serial, the text console and the display.
    /// Attention and fatal codes also go into the product activity log.
    pub fn emit(&mut self, src: &'static Src) {
        if self.role == Role::Storage {
            // slic shows, logs and records it with the reply.
            self.raised.push(src);
            return;
        }
        self.log.push(src);
        self.out.both(&src.line());
        let severity = match src.phase {
            crate::src::Phase::Attention => Some(licpart::Severity::Attention),
            crate::src::Phase::Fatal => Some(licpart::Severity::Fatal),
            _ => None,
        };
        if let Some(severity) = severity
            && let Some((part, record)) = self.lic.as_mut()
        {
            record.push_pal(licpart::PalEntry {
                time: now_unix(),
                code: src.bytes(),
                severity,
            });
            let _ = part.write(record);
        }
        self.show(Command::ShowSrc {
            system: self.system_name(),
            code: src.code.to_string(),
            text: src.text.to_string(),
        });
    }

    /// Send a command to the console thread, if there is one.
    pub fn show(&self, cmd: Command) {
        if let Some(tx) = &self.console {
            let _ = tx.send(cmd);
        }
    }

    /// QSYSNAME, or the default before system values are loaded.
    pub fn system_name(&self) -> String {
        self.sysvals
            .as_ref()
            .and_then(|v| v.get("QSYSNAME").ok())
            .map_or_else(|| "QS400".to_string(), |v| v.text())
    }

    /// Fatal: record the failure in the LIC and park forever.
    pub fn fatal(&mut self, src: &'static Src) -> ! {
        self.emit(src);
        lic::fail(self, src);
        self.out.both("slic: IPL halted\n");
        park()
    }
}

/// Seconds since the Unix epoch, 0 if the clock is before it.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
