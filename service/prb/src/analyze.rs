// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Problem analysis: the symptom strings and severities of detected
//! problems, and `ANZPRB`'s guided analysis of a reported one.

use crate::problem::Origin;

/// Message severity at and above which a `QSYSOPR` message is logged as
/// a problem.
pub const LOGGED_SEVERITY: u8 = 80;

/// A detected problem: severity 1..4, text, symptom string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// What raised it.
    pub origin: Origin,
    /// 1 (high) to 4 (low).
    pub severity: u8,
    /// Description.
    pub text: String,
    /// Symptom string: keyword/value pairs (`MSG/CPF0907 RSRC/DD001`).
    pub symptoms: String,
}

/// A severe message to `QSYSOPR`; `None` below the logged severity.
#[must_use]
pub fn from_message(msgid: &str, severity: u8, text: &str) -> Option<Detected> {
    (severity >= LOGGED_SEVERITY).then(|| Detected {
        origin: Origin::Message(msgid.into()),
        severity: if severity >= 99 { 1 } else { 2 },
        text: text.chars().take(50).collect(),
        symptoms: format!("MSG/{msgid}"),
    })
}

/// A device failure from the product activity log.
#[must_use]
pub fn from_hardware(resource: &str, src: &str, attention: bool) -> Detected {
    Detected {
        origin: Origin::Hardware {
            resource: resource.into(),
            src: src.into(),
        },
        severity: if attention { 2 } else { 3 },
        text: format!("Device {resource} reported {src}"),
        symptoms: format!("RSRC/{resource} SRC/{src}"),
    }
}

/// A system or job dump.
#[must_use]
pub fn from_dump(name: &str, why: &str) -> Detected {
    Detected {
        origin: Origin::Dump(name.into()),
        severity: 2,
        text: why.chars().take(50).collect(),
        symptoms: format!("DUMP/{name}"),
    }
}

/// The kinds of problem `ANZPRB` asks about first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// A job or program that ended, looped or waited.
    Job,
    /// A message the user saw.
    Message,
    /// A device: display, printer, tape, optical, disk.
    Device,
    /// Communications: a line, TCP/IP, a remote system.
    Communications,
    /// Performance.
    Performance,
}

impl Area {
    /// The areas in the order `ANZPRB`'s first panel numbers them.
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Job, "A job or program ended, looped or waited"),
        (Self::Message, "A message was shown"),
        (Self::Device, "A display, printer, tape, optical or disk device"),
        (Self::Communications, "Communications or a remote system"),
        (Self::Performance, "Performance"),
    ];

    /// From the option number (1 to 5).
    #[must_use]
    pub fn from_choice(n: u8) -> Option<Self> {
        Self::ALL.get(usize::from(n).checked_sub(1)?).map(|a| a.0)
    }
}

/// `ANZPRB`'s answers.
#[derive(Debug, Clone)]
pub struct Answers {
    /// The area chosen.
    pub area: Area,
    /// The message identifier, for a message problem.
    pub msgid: String,
    /// The job, device, line or program named.
    pub object: String,
    /// Whether it stops the user's work.
    pub stops_work: bool,
    /// What the user typed about it.
    pub description: String,
}

/// The problem a guided analysis produces.
#[must_use]
pub fn guided(a: &Answers) -> Detected {
    let (kw, sev_base) = match a.area {
        Area::Job => ("PGM", 2),
        Area::Message => ("MSG", 3),
        Area::Device => ("RSRC", 2),
        Area::Communications => ("LIND", 2),
        Area::Performance => ("PERF", 3),
    };
    let what = if a.area == Area::Message && !a.msgid.is_empty() {
        a.msgid.clone()
    } else {
        a.object.clone()
    };
    let mut symptoms = format!("{kw}/{what}");
    if !a.msgid.is_empty() && a.area != Area::Message {
        symptoms = format!("{symptoms} MSG/{}", a.msgid);
    }
    Detected {
        origin: Origin::Analysis,
        severity: if a.stops_work { 1 } else { sev_base },
        text: a.description.chars().take(50).collect(),
        symptoms,
    }
}
