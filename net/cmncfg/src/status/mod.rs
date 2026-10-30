// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime status of lines, their network controllers and devices:
//! what `WRKCFGSTS` shows. The service drives it with the vary
//! commands and the kernel's carrier events; it answers with the
//! messages to send ([`Effect`]). Statuses are not persisted: every
//! IPL starts varied off. A line recovers from a lost carrier within
//! its `CMNRCYLMT`: `RCYPND` while the carrier is awaited, `RCYCNL`
//! once more losses than the count fell in the interval, `FAILED`
//! when the count is 0.

mod line;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

pub use line::{Attached, LineConfig, LineState};

use crate::msgs;

/// A configuration object's status, as `WRKCFGSTS` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    /// Varied off.
    #[default]
    VariedOff,
    /// Vary on in progress.
    VaryOnPending,
    /// Varied on; no carrier yet.
    VariedOn,
    /// Carrier present, the controller contacted.
    Active,
    /// Carrier lost, no recovery.
    Failed,
    /// Carrier lost, recovery pending.
    RcyPnd,
    /// Carrier lost, recovery cancelled.
    RcyCnl,
    /// Vary off in progress.
    VaryOffPending,
}

impl Status {
    /// The status text.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Status::VariedOff => "VARIED OFF",
            Status::VaryOnPending => "VARY ON PENDING",
            Status::VariedOn => "VARIED ON",
            Status::Active => "ACTIVE",
            Status::Failed => "FAILED",
            Status::RcyPnd => "RCYPND",
            Status::RcyCnl => "RCYCNL",
            Status::VaryOffPending => "VARY OFF PENDING",
        }
    }

    /// The `RTVCFGSTS` code: 0 varied off, 10 vary off pending, 20
    /// vary on pending, 30 varied on, 60 active, 80 recovery pending,
    /// 90 recovery cancelled, 100 failed.
    #[must_use]
    pub fn code(self) -> u16 {
        match self {
            Status::VariedOff => 0,
            Status::VaryOffPending => 10,
            Status::VaryOnPending => 20,
            Status::VariedOn => 30,
            Status::Active => 60,
            Status::RcyPnd => 80,
            Status::RcyCnl => 90,
            Status::Failed => 100,
        }
    }

    /// Whether the object counts as varied on (a vary on is refused).
    #[must_use]
    pub fn is_on(self) -> bool {
        !matches!(self, Status::VariedOff)
    }

    /// Whether `WRKCFGSTS STATUS(filter)` shows it: `*ALL`, `*ACTIVE`,
    /// `*FAILED` (failed and recovery states), `*VARYON`, `*VARYOFF`,
    /// `*AVAILABLE` (varied off, nobody holds it).
    #[must_use]
    pub fn passes(self, filter: &str) -> bool {
        match filter {
            "*ACTIVE" => self == Status::Active,
            "*FAILED" => matches!(self, Status::Failed | Status::RcyPnd | Status::RcyCnl),
            "*VARYON" => self.is_on(),
            "*VARYOFF" | "*AVAILABLE" => !self.is_on(),
            _ => true,
        }
    }
}

/// What kind of object a [`Row`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `*LIN`.
    Line,
    /// `*CTL`.
    Ctl,
    /// `*DEV`.
    Dev,
}

impl Kind {
    /// `*LIN`, `*CTL`, `*DEV`.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Kind::Line => "*LIN",
            Kind::Ctl => "*CTL",
            Kind::Dev => "*DEV",
        }
    }

    /// From `*LIN`, `*CTL`, `*DEV`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "*LIN" => Some(Kind::Line),
            "*CTL" => Some(Kind::Ctl),
            "*DEV" => Some(Kind::Dev),
            _ => None,
        }
    }
}

/// One row of `WRKCFGSTS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The object's name.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// How far under the row above it: 0 for the object asked for.
    pub depth: u8,
    /// Its status.
    pub status: Status,
}

/// What the service must do after a change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Send a message to the line's queue.
    Message {
        /// `CPF5908`, `CPI5917`, `CPA58EE`.
        msgid: String,
        /// The line.
        line: String,
        /// Its network controller, blank for none.
        ctl: String,
        /// The queue as `LIB/NAME`.
        msgq: String,
        /// The message data, in the identifier's layout.
        data: Vec<u8>,
        /// An inquiry (replies `C G R`), not an informational message.
        inquiry: bool,
    },
}

/// Every line with its controllers and devices.
#[derive(Debug, Default)]
pub struct State {
    lines: BTreeMap<String, LineState>,
}

impl State {
    /// A state with no lines.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a line (varied off), or give one of the name its new
    /// description and keep its status and attachments.
    pub fn add_line(&mut self, name: &str, config: LineConfig) {
        let name = name.to_ascii_uppercase();
        if let Some(l) = self.lines.get_mut(&name) {
            l.config = config;
        } else {
            let l = LineState::new(&name, config);
            self.lines.insert(name, l);
        }
    }

    /// Forget a line and what hangs off it.
    pub fn remove_line(&mut self, name: &str) -> Option<LineState> {
        self.lines.remove(&name.to_ascii_uppercase())
    }

    /// The line called `name`.
    #[must_use]
    pub fn line(&self, name: &str) -> Option<&LineState> {
        self.lines.get(&name.to_ascii_uppercase())
    }

    /// The line called `name`, to change.
    pub fn line_mut(&mut self, name: &str) -> Option<&mut LineState> {
        self.lines.get_mut(&name.to_ascii_uppercase())
    }

    /// The lines, by name.
    pub fn lines(&self) -> impl Iterator<Item = &LineState> {
        self.lines.values()
    }

    /// The lines described `ONLINE(*YES)`, to vary on at IPL.
    #[must_use]
    pub fn online_lines(&self) -> Vec<String> {
        self.lines
            .values()
            .filter(|l| l.config.online)
            .map(|l| l.name.clone())
            .collect()
    }

    /// The line a controller is attached to.
    #[must_use]
    pub fn line_of_ctl(&self, ctl: &str) -> Option<&LineState> {
        let ctl = ctl.to_ascii_uppercase();
        self.lines
            .values()
            .find(|l| l.ctls.iter().any(|c| c.name == ctl))
    }

    /// The line and controller a device is attached to.
    #[must_use]
    pub fn line_of_dev(&self, dev: &str) -> Option<(&LineState, &Attached)> {
        let dev = dev.to_ascii_uppercase();
        self.lines.values().find_map(|l| {
            l.ctls
                .iter()
                .find(|c| c.devs.iter().any(|d| d.name == dev))
                .map(|c| (l, c))
        })
    }

    /// Whether a controller or device of this name is attached to some
    /// line (the name a new one may not take).
    #[must_use]
    pub fn attached(&self, name: &str) -> bool {
        self.line_of_ctl(name).is_some() || self.line_of_dev(name).is_some()
    }

    /// The status of an object of `kind`, `None` when no line has it.
    #[must_use]
    pub fn status_of(&self, kind: Kind, name: &str) -> Option<Status> {
        match kind {
            Kind::Line => self.line(name).map(|l| l.status),
            Kind::Ctl => {
                let l = self.line_of_ctl(name)?;
                let c = l
                    .ctls
                    .iter()
                    .find(|c| c.name == name.to_ascii_uppercase())?;
                Some(l.ctl_status(c))
            }
            Kind::Dev => {
                let (l, c) = self.line_of_dev(name)?;
                let d = c
                    .devs
                    .iter()
                    .find(|d| d.name == name.to_ascii_uppercase())?;
                Some(l.dev_status(c, d))
            }
        }
    }

    /// The line whose interface is `ifname`.
    fn line_of_ifname(&mut self, ifname: &str) -> Option<&mut LineState> {
        self.lines
            .values_mut()
            .find(|l| !l.ifname.is_empty() && l.ifname == ifname)
    }

    fn not_found(name: &str) -> String {
        format!("{} line description {name} not found", msgs::LINE_NOT_FOUND)
    }

    /// Begin a vary on: the line goes to `VARY ON PENDING`. Refused
    /// when it is not varied off.
    pub fn vary_on_begin(&mut self, name: &str) -> Result<(), String> {
        let l = self.line_mut(name).ok_or_else(|| Self::not_found(name))?;
        if l.status.is_on() {
            return Err(format!(
                "{} line {name} is varied on already ({})",
                msgs::LINE_ALREADY_ON,
                l.status.text()
            ));
        }
        l.status = Status::VaryOnPending;
        l.failures.clear();
        Ok(())
    }

    /// End a vary on: the kernel's answer, with the carrier when the
    /// link came up. `ACTIVE` with a carrier, else `VARIED ON`; a
    /// failure puts the line back to `VARIED OFF`. `RANGE(*NET)`
    /// varies the attached controllers and devices on too.
    pub fn vary_on_done(
        &mut self,
        name: &str,
        ifname: &str,
        outcome: Result<bool, String>,
        range_net: bool,
    ) -> Result<Vec<Effect>, String> {
        let l = self.line_mut(name).ok_or_else(|| Self::not_found(name))?;
        let carrier = match outcome {
            Ok(c) => c,
            Err(e) => {
                l.status = Status::VariedOff;
                l.ifname.clear();
                return Err(format!("{} line {name}: {e}", msgs::LINE_NOT_VARIED_ON));
            }
        };
        l.ifname = ifname.to_string();
        if range_net {
            for c in &mut l.ctls {
                c.on = true;
                for d in &mut c.devs {
                    d.on = true;
                }
            }
        }
        if carrier {
            l.status = Status::Active;
            Ok(vec![l.message(msgs::CONTACTED, false)])
        } else {
            l.status = Status::VariedOn;
            Ok(Vec::new())
        }
    }

    /// Vary a line off: `VARIED OFF` at once (the service takes the
    /// link down after). `RANGE(*NET)` takes the controllers and
    /// devices with it; without it they stay as they were, so a vary
    /// on brings them back. The interface the line had.
    pub fn vary_off(&mut self, name: &str, range_net: bool) -> Result<String, String> {
        let l = self.line_mut(name).ok_or_else(|| Self::not_found(name))?;
        if !l.status.is_on() {
            return Err(format!(
                "{} line {name} is varied off already",
                msgs::LINE_ALREADY_OFF
            ));
        }
        l.status = Status::VariedOff;
        l.failures.clear();
        if range_net {
            for c in &mut l.ctls {
                c.on = false;
                for d in &mut c.devs {
                    d.on = false;
                }
            }
        }
        Ok(std::mem::take(&mut l.ifname))
    }

    /// Vary a controller on or off (`RANGE(*NET)` takes its devices).
    pub fn vary_ctl(&mut self, name: &str, on: bool, range_net: bool) -> Result<(), String> {
        let name = name.to_ascii_uppercase();
        let l = self
            .lines
            .values_mut()
            .find(|l| l.ctls.iter().any(|c| c.name == name))
            .ok_or_else(|| format!("{} controller {name} not found", msgs::CTL_NOT_FOUND))?;
        if on && !l.status.is_on() {
            return Err(format!(
                "{} controller {name}: its line {} is varied off",
                msgs::LINE_NOT_ON,
                l.name
            ));
        }
        let c = l
            .ctls
            .iter_mut()
            .find(|c| c.name == name)
            .expect("found above");
        if !on && !c.on {
            return Err(format!(
                "{} controller {name} is varied off already",
                msgs::CTL_ALREADY_OFF
            ));
        }
        c.on = on;
        if range_net || !on {
            for d in &mut c.devs {
                d.on = on;
            }
        }
        Ok(())
    }

    /// Vary a device on or off.
    pub fn vary_dev(&mut self, name: &str, on: bool) -> Result<(), String> {
        let name = name.to_ascii_uppercase();
        for l in self.lines.values_mut() {
            let line_on = l.status.is_on();
            for c in &mut l.ctls {
                if let Some(d) = c.devs.iter_mut().find(|d| d.name == name) {
                    if on && !(line_on && c.on) {
                        return Err(format!(
                            "{} device {name}: its controller {} is not varied on",
                            msgs::CTL_NOT_ON,
                            c.name
                        ));
                    }
                    if !on && !d.on {
                        return Err(format!(
                            "{} device {name} is varied off already",
                            msgs::DEV_ALREADY_OFF
                        ));
                    }
                    d.on = on;
                    return Ok(());
                }
            }
        }
        Err(format!(
            "{} device description {name} not found",
            msgs::DEV_NOT_FOUND
        ))
    }

    /// The kernel reported `ifname`'s carrier `up` at `now` (Unix
    /// seconds). Lines on other interfaces are untouched.
    pub fn carrier(&mut self, ifname: &str, up: bool, now: u64) -> Vec<Effect> {
        let Some(l) = self.line_of_ifname(ifname) else {
            return Vec::new();
        };
        if up {
            match l.status {
                Status::VariedOn | Status::RcyPnd => {
                    l.status = Status::Active;
                    vec![l.message(msgs::CONTACTED, false)]
                }
                _ => Vec::new(),
            }
        } else {
            match l.status {
                Status::Active => l.fail(now),
                _ => Vec::new(),
            }
        }
    }

    /// The kernel removed `ifname` (the port went away): the line
    /// fails as on a lost carrier.
    pub fn link_removed(&mut self, ifname: &str, now: u64) -> Vec<Effect> {
        self.carrier(ifname, false, now)
    }

    /// The rows of `WRKCFGSTS CFGTYPE(kind)`: each object of the kind
    /// whose name passes `wanted`, with what hangs off it beneath when
    /// `range_net`, in name order of the objects asked for.
    #[must_use]
    pub fn rows(&self, kind: Kind, wanted: &dyn Fn(&str) -> bool, range_net: bool) -> Vec<Row> {
        let row = |name: &str, kind, depth, status| Row {
            name: name.to_string(),
            kind,
            depth,
            status,
        };
        let mut groups: Vec<(String, Vec<Row>)> = Vec::new();
        for l in self.lines.values() {
            match kind {
                Kind::Line if wanted(&l.name) => {
                    let mut g = vec![row(&l.name, Kind::Line, 0, l.status)];
                    if range_net {
                        for c in &l.ctls {
                            g.push(row(&c.name, Kind::Ctl, 1, l.ctl_status(c)));
                            for d in &c.devs {
                                g.push(row(&d.name, Kind::Dev, 2, l.dev_status(c, d)));
                            }
                        }
                    }
                    groups.push((l.name.clone(), g));
                }
                Kind::Line => {}
                Kind::Ctl => {
                    for c in l.ctls.iter().filter(|c| wanted(&c.name)) {
                        let mut g = vec![row(&c.name, Kind::Ctl, 0, l.ctl_status(c))];
                        if range_net {
                            for d in &c.devs {
                                g.push(row(&d.name, Kind::Dev, 1, l.dev_status(c, d)));
                            }
                        }
                        groups.push((c.name.clone(), g));
                    }
                }
                Kind::Dev => {
                    for c in &l.ctls {
                        for d in c.devs.iter().filter(|d| wanted(&d.name)) {
                            groups.push((
                                d.name.clone(),
                                vec![row(&d.name, Kind::Dev, 0, l.dev_status(c, d))],
                            ));
                        }
                    }
                }
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        groups.into_iter().flat_map(|(_, g)| g).collect()
    }
}
