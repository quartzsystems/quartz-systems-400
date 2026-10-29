// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LPR client (RFC 1179) of a remote output queue: `02 queue LF`
//! (receive a printer job), then the data file (`03 count SP name LF`,
//! the bytes, a zero octet) and the control file (`02 count SP name
//! LF`, the bytes, a zero octet), each step acknowledged by one zero
//! octet. The data file goes first, as BSD `lpr` does. The source port
//! is not a reserved one (721 to 731): binding one needs privilege, and
//! the LPDs a writer meets do not check.

pub mod control;

use std::io::{Read, Write};

use super::{Endpoint, Receipt, Sink, SinkError, SinkErrorKind};
use crate::job::PrintJob;
use crate::transform::Output;
use control::{Build, ControlFile};

/// LPD's port.
pub const PORT: u16 = 515;

/// `DESTTYPE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DestType {
    /// `*OTHER`: any LPD.
    #[default]
    Other,
    /// `*OS400`: an IBM i (or QS/400) LPD; the SCS goes untransformed
    /// with the attributes in the control file.
    Os400,
    /// `*OS400V2`: the same over LPR here (IBM's V2 transfer is SNA).
    Os400V2,
    /// `*PSF2`: a PSF/2 server.
    Psf2,
    /// `*S390`: a System/390.
    S390,
    /// `*NDS`: a Netware directory server.
    Nds,
}

impl DestType {
    /// From the special value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "*OTHER" => Self::Other,
            "*OS400" | "*IBMI" => Self::Os400,
            "*OS400V2" => Self::Os400V2,
            "*PSF2" => Self::Psf2,
            "*S390" => Self::S390,
            "*NDS" => Self::Nds,
            _ => return None,
        })
    }

    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Other => "*OTHER",
            Self::Os400 => "*OS400",
            Self::Os400V2 => "*OS400V2",
            Self::Psf2 => "*PSF2",
            Self::S390 => "*S390",
            Self::Nds => "*NDS",
        }
    }

    /// The code kept in the `*OUTQ`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Other => 0,
            Self::Os400 => 1,
            Self::Os400V2 => 2,
            Self::Psf2 => 3,
            Self::S390 => 4,
            Self::Nds => 5,
        }
    }

    /// From the code.
    #[must_use]
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Os400,
            2 => Self::Os400V2,
            3 => Self::Psf2,
            4 => Self::S390,
            5 => Self::Nds,
            _ => Self::Other,
        }
    }

    /// Whether the receiver is an IBM i that recreates the file: the
    /// data goes as SCS and the attributes go with it.
    #[must_use]
    pub fn ibm_i(self) -> bool {
        matches!(self, Self::Os400 | Self::Os400V2)
    }
}

/// `DESTOPT`, taken apart. IBM's documented words: `XAIX` (the LPD
/// prints several copies from one control file), `XAUTOQ` (an IBM i
/// receiver picks the output queue from the queue name); `*NOWAIT` and
/// `*USRDFNTXT` are the command's special values. Anything else is kept
/// and not acted on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DestOpt {
    /// `XAIX`.
    pub aix: bool,
    /// `XAUTOQ`.
    pub autoq: bool,
    /// `*NOWAIT`: do not wait for the last acknowledgement.
    pub nowait: bool,
    /// `*USRDFNTXT`.
    pub usrdfntxt: bool,
    /// Every other word, as given.
    pub other: Vec<String>,
}

impl DestOpt {
    /// From the parameter's value; `*NONE` and blank are no options.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        let mut d = Self::default();
        for w in s.split_whitespace() {
            match w.to_ascii_uppercase().as_str() {
                "*NONE" => {}
                "XAIX" => d.aix = true,
                "XAUTOQ" => d.autoq = true,
                "*NOWAIT" => d.nowait = true,
                "*USRDFNTXT" => d.usrdfntxt = true,
                _ => d.other.push(w.to_string()),
            }
        }
        d
    }
}

/// An LPD queue.
#[derive(Clone, Debug)]
pub struct LprSink {
    /// Where the LPD is.
    pub endpoint: Endpoint,
    /// The printer queue (`RMTPRTQ`, resolved).
    pub queue: String,
    /// This system's host name.
    pub local_host: String,
    /// `DESTTYPE`.
    pub desttype: DestType,
    /// `DESTOPT`.
    pub destopt: DestOpt,
    /// `SEPPAGE`: ask for a banner page.
    pub seppage: bool,
    /// The class (`C`), blank for none.
    pub class: String,
}

impl LprSink {
    /// A queue at `host`, port 515.
    #[must_use]
    pub fn new(host: &str, queue: &str, local_host: &str) -> Self {
        Self {
            endpoint: Endpoint::new(host, PORT),
            queue: queue.into(),
            local_host: local_host.into(),
            desttype: DestType::Other,
            destopt: DestOpt::default(),
            seppage: true,
            class: String::new(),
        }
    }

    /// The job number of `job` on the wire: three digits.
    fn number(job: &PrintJob) -> u16 {
        u16::try_from((job.job.2.wrapping_mul(10).wrapping_add(job.number)) % 1000).unwrap_or(0)
    }

    /// The control files and how many times the job is sent: one job
    /// with the copies as print commands (`XAIX`) or as an attribute
    /// (`*OS400`), else one job per copy.
    #[must_use]
    pub fn plan(&self, job: &PrintJob) -> (ControlFile, u16) {
        let copies = job.copies.max(1);
        let ibm = self.desttype.ibm_i();
        let (prints, sends) = if ibm {
            (1, 1)
        } else if self.destopt.aix {
            (copies, 1)
        } else {
            (1, copies)
        };
        let cf = ControlFile::build(
            job,
            &Build {
                host: &self.local_host,
                number: Self::number(job),
                banner: self.seppage,
                prints,
                attributes: ibm,
                class: &self.class,
            },
        );
        (cf, sends)
    }

    fn ack(s: &mut impl Read, what: &str) -> Result<(), SinkError> {
        let mut b = [0u8; 1];
        match s.read(&mut b) {
            Ok(1) if b[0] == 0 => Ok(()),
            Ok(1) => Err(SinkError::new(
                SinkErrorKind::Rejected,
                format!("LPD refused {what} (code {})", b[0]),
            )),
            Ok(_) => Err(SinkError::new(
                SinkErrorKind::Io,
                format!("LPD closed the connection at {what}"),
            )),
            Err(e) => Err(SinkError::io(&e)),
        }
    }

    fn file(
        s: &mut (impl Read + Write),
        code: u8,
        name: &str,
        data: &[u8],
        last: bool,
    ) -> Result<(), SinkError> {
        let head = format!("{}{} {name}\n", char::from(code), data.len());
        s.write_all(head.as_bytes()).map_err(|e| SinkError::io(&e))?;
        Self::ack(s, name)?;
        s.write_all(data).map_err(|e| SinkError::io(&e))?;
        s.write_all(&[0]).map_err(|e| SinkError::io(&e))?;
        s.flush().map_err(|e| SinkError::io(&e))?;
        if last {
            return Ok(());
        }
        Self::ack(s, name)
    }

    fn send_once(&self, cf: &ControlFile, data: &[u8], job: &PrintJob) -> Result<(), SinkError> {
        let mut s = self.endpoint.connect()?;
        let n = Self::number(job);
        s.write_all(format!("\x02{}\n", self.queue).as_bytes())
            .map_err(|e| SinkError::io(&e))?;
        Self::ack(&mut s, &format!("queue {}", self.queue))?;
        Self::file(&mut s, 3, &control::data_name(&self.local_host, n), data, false)?;
        Self::file(
            &mut s,
            2,
            &control::control_name(&self.local_host, n),
            &cf.encode(),
            self.destopt.nowait,
        )
    }
}

impl Sink for LprSink {
    fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError> {
        let (cf, sends) = self.plan(job);
        for _ in 0..sends {
            self.send_once(&cf, &doc.data, job)?;
        }
        Ok(Receipt {
            bytes: doc.data.len() as u64 * u64::from(sends),
            job_id: Some(format!("{:03}", Self::number(job))),
            detail: format!(
                "queue {} on {}, {} {}",
                self.queue,
                self.endpoint.host,
                sends,
                if sends == 1 { "job" } else { "jobs" }
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_options() {
        let d = DestOpt::parse("xaix XAUTOQ -odouble");
        assert!(d.aix && d.autoq && !d.nowait);
        assert_eq!(d.other, vec!["-odouble".to_string()]);
        assert_eq!(DestOpt::parse("*NONE"), DestOpt::default());
        assert_eq!(DestType::parse("*ibmi"), Some(DestType::Os400));
        assert_eq!(DestType::from_code(DestType::Nds.code()), DestType::Nds);
    }

    #[test]
    fn copies_by_destination() {
        let job = PrintJob {
            copies: 3,
            ..PrintJob::default()
        };
        let mut s = LprSink::new("h", "Q", "me");
        let (cf, n) = s.plan(&job);
        assert_eq!((cf.prints().len(), n), (1, 3));
        s.destopt.aix = true;
        let (cf, n) = s.plan(&job);
        assert_eq!((cf.prints().len(), n), (3, 1));
        s.desttype = DestType::Os400;
        let (cf, n) = s.plan(&job);
        assert_eq!((cf.prints().len(), n), (1, 1));
        assert_eq!(cf.job().unwrap().copies, 3);
    }
}
