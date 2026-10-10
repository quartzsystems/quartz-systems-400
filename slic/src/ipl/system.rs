// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Everything the IPL and the service console share.

use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use licpart::{Device, IplMode, IplType, LicPart, Record, Slot};
use sysval::SysVals;

use console::Command;

use crate::hwrsrc::Table;
use crate::ipl::cmdline::Boot;
use crate::ipl::log::IplLog;
use crate::ipl::{lic, park};
use crate::net::Link;
use crate::platform::console::Outputs;
use crate::src::Src;

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
    pub fn emit(&mut self, src: &'static Src) {
        self.log.push(src);
        self.out.both(&src.line());
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
