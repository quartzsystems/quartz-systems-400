// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One line's runtime state: its status, the recovery limit and the
//! failures counted against it, and the controllers and devices
//! attached to it.

use super::{Effect, Status};
use crate::msgs;

/// What the state machine needs of a line description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineConfig {
    /// `CMNRCYLMT` resolved: attempts and the minutes they count in.
    pub rcy_count: u16,
    /// The interval, minutes; 0 counts every failure since vary on.
    pub rcy_interval: u16,
    /// The message queue as `LIB/NAME`.
    pub msgq: String,
    /// `ONLINE(*YES)`.
    pub online: bool,
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            rcy_count: 0,
            rcy_interval: 0,
            msgq: "QSYS/QSYSOPR".into(),
            online: true,
        }
    }
}

/// A controller or device attached under a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    /// Its name.
    pub name: String,
    /// Varied on (its own switch; the line's status decides the rest).
    pub on: bool,
    /// The devices under a controller (empty for a device).
    pub devs: Vec<Attached>,
}

impl Attached {
    /// A new attachment, varied off.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            on: false,
            devs: Vec::new(),
        }
    }
}

/// A line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineState {
    /// The line description's name.
    pub name: String,
    /// The kernel interface it is varied on over; blank until then.
    pub ifname: String,
    /// Its status.
    pub status: Status,
    /// What the description says.
    pub config: LineConfig,
    /// Unix seconds of the carrier losses counted against the limit.
    pub failures: Vec<u64>,
    /// The controllers, each with its devices.
    pub ctls: Vec<Attached>,
}

impl LineState {
    /// A line varied off with no interface yet.
    #[must_use]
    pub fn new(name: &str, config: LineConfig) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            ifname: String::new(),
            status: Status::VariedOff,
            config,
            failures: Vec::new(),
            ctls: Vec::new(),
        }
    }

    /// Attach a controller, or find it.
    pub fn attach_ctl(&mut self, name: &str) -> &mut Attached {
        let name = name.to_ascii_uppercase();
        if let Some(i) = self.ctls.iter().position(|c| c.name == name) {
            return &mut self.ctls[i];
        }
        self.ctls.push(Attached::new(&name));
        self.ctls.last_mut().expect("just pushed")
    }

    /// Attach a device under a controller (attached if new).
    pub fn attach_dev(&mut self, ctl: &str, dev: &str) {
        let c = self.attach_ctl(ctl);
        let dev = dev.to_ascii_uppercase();
        if !c.devs.iter().any(|d| d.name == dev) {
            c.devs.push(Attached::new(&dev));
        }
    }

    /// The first controller's name, blank for none.
    #[must_use]
    pub fn ctl_name(&self) -> String {
        self.ctls
            .first()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }

    /// A controller's status: varied off on its own switch, else the
    /// line's.
    #[must_use]
    pub fn ctl_status(&self, c: &Attached) -> Status {
        if !c.on || !self.status.is_on() {
            Status::VariedOff
        } else {
            self.status
        }
    }

    /// A device's status: off on its own or its controller's switch,
    /// else the line's.
    #[must_use]
    pub fn dev_status(&self, c: &Attached, d: &Attached) -> Status {
        if d.on {
            self.ctl_status(c)
        } else {
            Status::VariedOff
        }
    }

    /// A message effect for this line.
    pub(super) fn message(&self, msgid: &str, inquiry: bool) -> Effect {
        let ctl = self.ctl_name();
        Effect::Message {
            msgid: msgid.to_string(),
            data: msgs::data(msgid, &self.name, &ctl),
            line: self.name.clone(),
            ctl,
            msgq: self.config.msgq.clone(),
            inquiry,
        }
    }

    /// The carrier was lost at `now`: count the failure against the
    /// limit and pick the status.
    pub(super) fn fail(&mut self, now: u64) -> Vec<Effect> {
        let window = u64::from(self.config.rcy_interval) * 60;
        if window > 0 {
            self.failures.retain(|t| now.saturating_sub(*t) < window);
        }
        self.failures.push(now);
        let count = usize::from(self.config.rcy_count);
        if count == 0 {
            self.status = Status::Failed;
            vec![self.message(msgs::LINE_FAILED, true)]
        } else if self.failures.len() <= count {
            self.status = Status::RcyPnd;
            vec![self.message(msgs::RECOVERY_STARTED, false)]
        } else {
            self.status = Status::RcyCnl;
            vec![self.message(msgs::LINE_FAILED, true)]
        }
    }
}
