// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a writer job is told to do next.

use ipc::wire_enum;

use crate::obj::splf::SplfAttrs;

/// The writer's next step.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // one at a time, over the wire
pub enum WtrStep {
    /// End: `ENDWTR`, or `AUTOEND(*YES)` with the queue empty.
    End,
    /// Nothing to print now (held, or the queue is empty or held).
    Wait,
    /// Print this file.
    File {
        /// Its attributes.
        attrs: SplfAttrs,
        /// Its SCS.
        data: Vec<u8>,
    },
    /// The writer's inquiry waits for its reply (`MSGW`).
    MsgWait,
}

wire_enum!(WtrStep {
    0 => End,
    1 => Wait,
    2 => File { attrs, data },
    3 => MsgWait,
});

/// A reply to the writer's inquiry after a printer error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WtrReply {
    /// `R`: print the file again from the start.
    Retry,
    /// `H`: hold the file and go on with the next one.
    Hold,
    /// `C`: end the writer; the file stays ready.
    Cancel,
}

impl WtrReply {
    /// The reply typed, any case; `None` when not one of `C H R`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "R" => Some(Self::Retry),
            "H" => Some(Self::Hold),
            "C" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// Where a writer prints, as the service tells the writer job when it
/// starts (`docs/PRINT.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WtrDest {
    /// The capture device: printed into the store.
    Capture,
    /// A LAN printer device (`CRTDEVPRT DEVCLS(*LAN) LANATTACH(*IP)`).
    Lan {
        /// `RMTLOCNAME`.
        host: String,
        /// `PORT`; 0 for the driver's.
        port: u16,
        /// `SYSDRVPGM`.
        driver: String,
        /// `SECURECNN(*YES)`.
        secure: bool,
        /// `TRANSFORM(*YES)`.
        transform: bool,
        /// `MFRTYPMDL`.
        mfrtypmdl: String,
        /// The `*WSCST` payload for `MFRTYPMDL(*WSCST)`; empty for none.
        wscst: Vec<u8>,
        /// `CHRID`: `gcgid cpgid` or `*SYSVAL`.
        chrid: String,
    },
    /// A remote output queue (`STRRMTWTR`).
    Remote {
        /// The host: `RMTSYS`, or `INTNETADR`.
        host: String,
        /// `RMTPRTQ`; blank for `*USER`.
        queue: String,
        /// `DESTTYPE` code (`wm::obj::outqrmt::DESTTYPE`).
        desttype: u8,
        /// `DESTOPT`.
        destopt: String,
        /// `SEPPAGE(*YES)`.
        seppage: bool,
        /// `TRANSFORM(*YES)`.
        transform: bool,
        /// `MFRTYPMDL`.
        mfrtypmdl: String,
        /// The `*WSCST` payload for `MFRTYPMDL(*WSCST)`; empty for none.
        wscst: Vec<u8>,
        /// This system's name, for the LPR host lines.
        local_host: String,
    },
    /// A virtual printer a TN5250E printer session holds (RFC 2877).
    Session {
        /// The device; its session listens at `dev::socket(device)`.
        device: String,
    },
}

wire_enum!(WtrDest {
    0 => Capture,
    1 => Lan { host, port, driver, secure, transform, mfrtypmdl, wscst, chrid },
    2 => Remote { host, queue, desttype, destopt, seppage, transform, mfrtypmdl, wscst, local_host },
    3 => Session { device },
});

/// A started writer, as the service keeps it.
#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Writer {
    /// The device.
    pub dev: String,
    /// The queue it prints, `lib/name`.
    pub outq: String,
    /// Its job's number.
    pub job: u32,
    /// `HLDWTR`.
    pub held: bool,
    /// `ENDWTR`: end after the file in hand.
    pub ending: bool,
    /// `FORMTYPE`: `*ALL` or a form type.
    pub formtype: String,
    /// `FILESEP`.
    pub filesep: u16,
    /// `AUTOEND(*YES)`: end when the queue is empty.
    pub autoend: bool,
    /// The file being printed.
    pub current: Option<u32>,
    /// Files printed since `STRPRTWTR`: each counted once, when the
    /// device has taken all of it.
    pub printed: u32,
    /// A remote writer (`STRRMTWTR`): it prints a remote output queue.
    pub remote: bool,
    /// The message it waits on after a failed send.
    pub msgw: Option<String>,
    /// The inquiry it waits on: the queue `lib/name`, the message key
    /// and the spooled file it is about.
    pub inquiry: Option<(String, u32, u32)>,
}

/// What a writer does once its inquiry is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterReply {
    /// Go on: the file is ready and prints again.
    Go,
    /// Hold this spooled file, then go on.
    Hold(u32),
    /// End the writer.
    End,
}

impl Writer {
    /// A writer just started: nothing printed yet.
    #[must_use]
    pub fn started(dev: &str, outq: &str, job: u32) -> Self {
        Self {
            dev: dev.to_string(),
            outq: outq.to_string(),
            job,
            formtype: "*ALL".into(),
            ..Self::default()
        }
    }

    /// The device took the whole file: counted once.
    pub fn completed(&mut self) {
        self.current = None;
        self.msgw = None;
        self.printed += 1;
    }

    /// The file in hand did not print; nothing is counted.
    pub fn stopped(&mut self) {
        self.current = None;
    }

    /// The device refused the file: the writer waits on inquiry `key`
    /// on `queue` about spooled file `file`.
    pub fn ask(&mut self, msgid: &str, queue: &str, key: u32, file: u32) {
        self.stopped();
        self.msgw = Some(msgid.to_string());
        self.inquiry = Some((queue.to_string(), key, file));
    }

    /// The inquiry was answered `reply` (blank or unknown: the default,
    /// retry).
    pub fn answered(&mut self, reply: &str) -> AfterReply {
        let file = self.inquiry.take().map_or(0, |(_, _, f)| f);
        self.msgw = None;
        match WtrReply::parse(reply).unwrap_or(WtrReply::Retry) {
            WtrReply::Retry => AfterReply::Go,
            WtrReply::Hold => AfterReply::Hold(file),
            WtrReply::Cancel => {
                self.ending = true;
                AfterReply::End
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_counts_completions_only() {
        let mut w = Writer::started("QS01P01", "QUSRSYS/QS01P01", 7);
        assert_eq!(w.printed, 0);
        w.current = Some(1);
        w.stopped();
        w.current = Some(1);
        w.ask("CPA3388", "QSYS/QSYSOPR", 9, 1);
        assert_eq!(w.printed, 0);
        assert_eq!(w.msgw.as_deref(), Some("CPA3388"));
        assert_eq!(w.answered("R"), AfterReply::Go);
        w.current = Some(1);
        w.completed();
        w.current = Some(2);
        w.completed();
        assert_eq!(w.printed, 2);
        assert!(w.current.is_none());
        // STRPRTWTR starts the count again.
        assert_eq!(Writer::started("QS01P01", "QUSRSYS/QS01P01", 8).printed, 0);
    }

    #[test]
    fn replies_to_the_inquiry() {
        let mut w = Writer::started("D", "QUSRSYS/D", 1);
        w.ask("CPA3388", "QSYS/QSYSOPR", 4, 33);
        assert_eq!(w.answered("h"), AfterReply::Hold(33));
        assert!(w.inquiry.is_none() && w.msgw.is_none());
        w.ask("CPA3388", "QSYS/QSYSOPR", 5, 34);
        assert_eq!(w.answered(""), AfterReply::Go);
        w.ask("CPA3388", "QSYS/QSYSOPR", 6, 35);
        assert_eq!(w.answered("C"), AfterReply::End);
        assert!(w.ending);
    }

    #[test]
    fn replies() {
        assert_eq!(WtrReply::parse(" r "), Some(WtrReply::Retry));
        assert_eq!(WtrReply::parse("H"), Some(WtrReply::Hold));
        assert_eq!(WtrReply::parse("c"), Some(WtrReply::Cancel));
        assert_eq!(WtrReply::parse("G"), None);
    }

    #[test]
    fn steps_and_targets_cross_the_wire() {
        for s in [WtrStep::End, WtrStep::Wait, WtrStep::MsgWait] {
            let b = ipc::encode(&s);
            assert_eq!(ipc::decode::<WtrStep>(&b).unwrap(), s);
        }
        let d = WtrDest::Session { device: "QS01P01".into() };
        assert_eq!(ipc::decode::<WtrDest>(&ipc::encode(&d)).unwrap(), d);
    }
}
