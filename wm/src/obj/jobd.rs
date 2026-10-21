// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*JOBD`: the attributes a job starts with. Values that may be special
//! (`*USRPRF`, `*SYSVAL`, `*RQD` ...) are kept as their text; the job
//! start resolves them.

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload's fixed part.
pub const LAYOUT: Layout = Layout {
    name: "JobDescription",
    size: Some(352),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("LiblSysval", 10, 1),
        field("Hold", 11, 1),
        field("User", 12, 10),
        field("JobqName", 22, 10),
        field("JobqLib", 32, 10),
        field("JobPty", 42, 1),
        field("OutPty", 43, 1),
        field("Syntax", 44, 1),
        field("EndSev", 45, 1),
        field("LogLevel", 46, 1),
        field("LogSev", 47, 1),
        field("LogText", 48, 10),
        field("LogClPgm", 58, 10),
        field("LogOutput", 68, 10),
        field("InqMsgRpy", 78, 10),
        field("PrtDev", 88, 10),
        field("OutqName", 98, 10),
        field("OutqLib", 108, 10),
        field("InlAspGrp", 118, 10),
        field("DevRcyAcn", 128, 13),
        field("AlwMltThd", 141, 1),
        field("TsePool", 142, 10),
        field("JobMsgQFl", 152, 10),
        field("JobMsgQMx", 162, 2),
        field("SplfAcn", 164, 10),
        field("DdmCnv", 174, 10),
        field("Reserved", 184, 4),
        field("PrtTxt", 188, 30),
        field("RtgDta", 218, 80),
        field("Text", 298, 50),
        field("RqsDtaLen", 348, 4),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMJOBD";
const VERSION: u64 = 1;
/// `SYNTAX(*NOCHK)`.
const NOCHK: u64 = 0xFF;

/// A job description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobD {
    /// `USER`: a profile or `*RQD`.
    pub user: String,
    /// `JOBQ`: library and name.
    pub jobq: (String, String),
    /// `JOBPTY` 0-9.
    pub jobpty: u8,
    /// `OUTPTY` 0-9.
    pub outpty: u8,
    /// `PRTDEV`: a device, `*USRPRF`, `*SYSVAL`, `*WRKSTN`.
    pub prtdev: String,
    /// `OUTQ`: library and name, or `*USRPRF` ... as the name.
    pub outq: (String, String),
    /// `PRTTXT` or `*SYSVAL`.
    pub prttxt: String,
    /// `RTGDTA`.
    pub rtgdta: String,
    /// `RQSDTA` or `*NONE`.
    pub rqsdta: String,
    /// `INLLIBL`: `None` is `*SYSVAL`.
    pub inllibl: Option<Vec<String>>,
    /// `INLASPGRP`.
    pub inlaspgrp: String,
    /// `LOG` level 0-4.
    pub log_level: u8,
    /// `LOG` severity.
    pub log_sev: u8,
    /// `LOG` text: `*MSG`, `*SECLVL`, `*NOLIST`.
    pub log_text: String,
    /// `LOGCLPGM`: `*YES`, `*NO`.
    pub logclpgm: String,
    /// `LOGOUTPUT`: `*SYSVAL`, `*JOBLOGSVR`, `*JOBEND`, `*PND`.
    pub logoutput: String,
    /// `INQMSGRPY`: `*RQD`, `*DFT`, `*SYSRPYL`.
    pub inqmsgrpy: String,
    /// `HOLD`.
    pub hold: bool,
    /// `DEVRCYACN`.
    pub devrcyacn: String,
    /// `TSEPOOL`.
    pub tsepool: String,
    /// `ENDSEV`.
    pub endsev: u8,
    /// `SYNTAX`: `None` is `*NOCHK`.
    pub syntax: Option<u8>,
    /// `JOBMSGQMX` in MB: `None` is `*SYSVAL`.
    pub jobmsgqmx: Option<u16>,
    /// `JOBMSGQFL`.
    pub jobmsgqfl: String,
    /// `ALWMLTTHD`.
    pub alwmltthd: bool,
    /// `SPLFACN`.
    pub splfacn: String,
    /// `DDMCNV`.
    pub ddmcnv: String,
    /// `TEXT`.
    pub text: String,
}

impl Default for JobD {
    /// `CRTJOBD`'s defaults.
    fn default() -> Self {
        Self {
            user: "*RQD".into(),
            jobq: ("QGPL".into(), "QBATCH".into()),
            jobpty: 5,
            outpty: 5,
            prtdev: "*USRPRF".into(),
            outq: (String::new(), "*USRPRF".into()),
            prttxt: "*SYSVAL".into(),
            rtgdta: "QCMDI".into(),
            rqsdta: "*NONE".into(),
            inllibl: None,
            inlaspgrp: "*NONE".into(),
            log_level: 4,
            log_sev: 0,
            log_text: "*NOLIST".into(),
            logclpgm: "*NO".into(),
            logoutput: "*SYSVAL".into(),
            inqmsgrpy: "*RQD".into(),
            hold: false,
            devrcyacn: "*SYSVAL".into(),
            tsepool: "*SYSVAL".into(),
            endsev: 30,
            syntax: None,
            jobmsgqmx: None,
            jobmsgqfl: "*SYSVAL".into(),
            alwmltthd: false,
            splfacn: "*SYSVAL".into(),
            ddmcnv: "*KEEP".into(),
            text: String::new(),
        }
    }
}

impl JobD {
    /// The payload: the fixed part, the library list, the request data.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("LiblSysval", self.inllibl.is_none());
        r.set_flag("Hold", self.hold);
        r.set_text("User", &self.user);
        r.set_text("JobqName", &self.jobq.1);
        r.set_text("JobqLib", &self.jobq.0);
        r.set_uint("JobPty", u64::from(self.jobpty));
        r.set_uint("OutPty", u64::from(self.outpty));
        r.set_uint("Syntax", self.syntax.map_or(NOCHK, u64::from));
        r.set_uint("EndSev", u64::from(self.endsev));
        r.set_uint("LogLevel", u64::from(self.log_level));
        r.set_uint("LogSev", u64::from(self.log_sev));
        r.set_text("LogText", &self.log_text);
        r.set_text("LogClPgm", &self.logclpgm);
        r.set_text("LogOutput", &self.logoutput);
        r.set_text("InqMsgRpy", &self.inqmsgrpy);
        r.set_text("PrtDev", &self.prtdev);
        r.set_text("OutqName", &self.outq.1);
        r.set_text("OutqLib", &self.outq.0);
        r.set_text("InlAspGrp", &self.inlaspgrp);
        r.set_text("DevRcyAcn", &self.devrcyacn);
        r.set_flag("AlwMltThd", self.alwmltthd);
        r.set_text("TsePool", &self.tsepool);
        r.set_text("JobMsgQFl", &self.jobmsgqfl);
        r.set_uint("JobMsgQMx", u64::from(self.jobmsgqmx.unwrap_or(0)));
        r.set_text("SplfAcn", &self.splfacn);
        r.set_text("DdmCnv", &self.ddmcnv);
        r.set_text("PrtTxt", &self.prttxt);
        r.set_text("RtgDta", &self.rtgdta);
        r.set_text("Text", &self.text);
        let rqs = crate::rec::text_bytes(&self.rqsdta, self.rqsdta.chars().count());
        r.set_uint("RqsDtaLen", rqs.len() as u64);
        let mut out = r.buf;
        super::put_names(&mut out, self.inllibl.as_deref().unwrap_or(&[]));
        out.extend(rqs);
        out
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a job description".into()));
        }
        let (libl, used) = super::get_names(r.tail())?;
        let n = usize::try_from(r.uint("RqsDtaLen")).unwrap_or(usize::MAX);
        let rqs = r
            .tail()
            .get(used..used.saturating_add(n))
            .ok_or_else(|| Error::Corrupt("request data truncated".into()))?;
        let small = |name: &str| u8::try_from(r.uint(name)).unwrap_or(u8::MAX);
        Ok(Self {
            user: r.text("User"),
            jobq: (r.text("JobqLib"), r.text("JobqName")),
            jobpty: small("JobPty"),
            outpty: small("OutPty"),
            prtdev: r.text("PrtDev"),
            outq: (r.text("OutqLib"), r.text("OutqName")),
            prttxt: r.text("PrtTxt"),
            rtgdta: r.text("RtgDta"),
            rqsdta: crate::rec::text_of(rqs),
            inllibl: (!r.flag("LiblSysval")).then_some(libl),
            inlaspgrp: r.text("InlAspGrp"),
            log_level: small("LogLevel"),
            log_sev: small("LogSev"),
            log_text: r.text("LogText"),
            logclpgm: r.text("LogClPgm"),
            logoutput: r.text("LogOutput"),
            inqmsgrpy: r.text("InqMsgRpy"),
            hold: r.flag("Hold"),
            devrcyacn: r.text("DevRcyAcn"),
            tsepool: r.text("TsePool"),
            endsev: small("EndSev"),
            syntax: (r.uint("Syntax") != NOCHK).then(|| small("Syntax")),
            jobmsgqmx: match r.uint("JobMsgQMx") {
                0 => None,
                n => u16::try_from(n).ok(),
            },
            jobmsgqfl: r.text("JobMsgQFl"),
            alwmltthd: r.flag("AlwMltThd"),
            splfacn: r.text("SplfAcn"),
            ddmcnv: r.text("DdmCnv"),
            text: r.text("Text"),
        })
    }

    /// Check the values a command could get wrong.
    pub fn validate(&self) -> Result<()> {
        if self.jobpty > 9 || self.outpty > 9 {
            return Err(Error::value("CPD0085", "a priority is 0 through 9"));
        }
        if self.log_level > 4 || self.log_sev > 99 || self.endsev > 99 {
            return Err(Error::value(
                "CPD0085",
                "a log level is 0 through 4, a severity 0 through 99",
            ));
        }
        if !matches!(self.log_text.as_str(), "*MSG" | "*SECLVL" | "*NOLIST") {
            return Err(Error::value(
                "CPD0084",
                format!("LOG text {}", self.log_text),
            ));
        }
        if !matches!(self.inqmsgrpy.as_str(), "*RQD" | "*DFT" | "*SYSRPYL") {
            return Err(Error::value(
                "CPD0084",
                format!("INQMSGRPY({})", self.inqmsgrpy),
            ));
        }
        if !matches!(
            self.logoutput.as_str(),
            "*SYSVAL" | "*JOBLOGSVR" | "*JOBEND" | "*PND"
        ) {
            return Err(Error::value(
                "CPD0084",
                format!("LOGOUTPUT({})", self.logoutput),
            ));
        }
        if self.inllibl.as_ref().is_some_and(|l| l.len() > 250) {
            return Err(Error::value("CPD0085", "at most 250 libraries"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let d = JobD::default();
        assert_eq!(JobD::decode(&d.encode()).unwrap(), d);
        let e = JobD {
            user: "QSECOFR".into(),
            inllibl: Some(vec!["QGPL".into(), "QTEMP".into()]),
            rqsdta: "CALL QGPL/X PARM('a b')".into(),
            syntax: Some(20),
            jobmsgqmx: Some(16),
            hold: true,
            text: "Test description".into(),
            ..JobD::default()
        };
        assert_eq!(JobD::decode(&e.encode()).unwrap(), e);
        assert_eq!(e.encode().len(), 352 + 2 + 20 + e.rqsdta.len());
        assert!(JobD::decode(b"short").is_err());
        assert!(
            JobD {
                jobpty: 10,
                ..JobD::default()
            }
            .validate()
            .is_err()
        );
    }
}
