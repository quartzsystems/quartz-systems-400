// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*LIND` payload: a fixed record (`docs/TCPIP.md`,
//! `LineDescription`) followed by the aggregated resource list and the
//! extra parameters as `KWD=value;` text in CP037.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use wm::rec::{Layout, Rec, field};

use super::{AUTODLTCTL_NONE, Lind, RcyLimit};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "LineDescription",
    size: Some(272),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Online", 10, 1),
        field("AutoCrtCtl", 11, 1),
        field("GenTstFrm", 12, 1),
        field("Reserved", 13, 1),
        field("VryWait", 14, 2),
        field("RsrcName", 16, 10),
        field("Bridge", 26, 10),
        field("AggStd", 36, 10),
        field("AggPcy", 46, 10),
        field("NwsName", 56, 10),
        field("NwsPort", 66, 2),
        field("AssocPort", 68, 10),
        field("AdptAdr", 78, 12),
        field("EthStd", 90, 10),
        field("LineSpeed", 100, 10),
        field("Duplex", 110, 10),
        field("MaxFrame", 120, 4),
        field("MaxCtl", 124, 2),
        field("ExchId", 126, 8),
        field("Threshold", 134, 10),
        field("LinkSpeed", 144, 10),
        field("CostCnn", 154, 1),
        field("CostByte", 155, 1),
        field("Security", 156, 10),
        field("PrpDly", 166, 10),
        field("UsrDfn1", 176, 1),
        field("UsrDfn2", 177, 1),
        field("UsrDfn3", 178, 1),
        field("Reserved2", 179, 1),
        field("AutoDltCtl", 180, 4),
        field("RcyCount", 184, 2),
        field("RcyInterval", 186, 2),
        field("MsgqName", 188, 10),
        field("MsgqLib", 198, 10),
        field("Text", 208, 50),
        field("Reserved3", 258, 14),
    ],
};

const MAGIC: &[u8; 8] = b"QSCMLIND";
const VERSION: u64 = 1;

/// `*SYSVAL` in `RcyCount`.
const RCY_SYSVAL: u64 = 0xFFFF;

impl Lind {
    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Online", self.online);
        r.set_flag("AutoCrtCtl", self.autocrtctl);
        r.set_flag("GenTstFrm", self.gentstfrm);
        r.set_uint("VryWait", u64::from(self.vrywait));
        r.set_text("RsrcName", &self.rsrcname);
        r.set_text("Bridge", &self.bridge);
        r.set_text("AggStd", &self.aggpcy.0);
        r.set_text("AggPcy", &self.aggpcy.1);
        r.set_text("NwsName", &self.nws.0);
        r.set_uint("NwsPort", u64::from(self.nws.1));
        r.set_text("AssocPort", &self.assocport);
        r.set_text("AdptAdr", &self.adptadr);
        r.set_text("EthStd", &self.ethstd);
        r.set_text("LineSpeed", &self.linespeed);
        r.set_text("Duplex", &self.duplex);
        r.set_uint("MaxFrame", u64::from(self.maxframe));
        r.set_uint("MaxCtl", u64::from(self.maxctl));
        r.set_text("ExchId", &self.exchid);
        r.set_text("Threshold", &self.threshold);
        r.set_text("LinkSpeed", &self.linkspeed);
        r.set_uint("CostCnn", u64::from(self.costcnn));
        r.set_uint("CostByte", u64::from(self.costbyte));
        r.set_text("Security", &self.security);
        r.set_text("PrpDly", &self.prpdly);
        r.set_uint("UsrDfn1", u64::from(self.usrdfn[0]));
        r.set_uint("UsrDfn2", u64::from(self.usrdfn[1]));
        r.set_uint("UsrDfn3", u64::from(self.usrdfn[2]));
        r.set_uint("AutoDltCtl", u64::from(self.autodltctl));
        match self.cmnrcylmt {
            RcyLimit::SysVal => {
                r.set_uint("RcyCount", RCY_SYSVAL);
                r.set_uint("RcyInterval", 0);
            }
            RcyLimit::Limit { count, interval } => {
                r.set_uint("RcyCount", u64::from(count));
                r.set_uint("RcyInterval", u64::from(interval));
            }
        }
        let (lib, name) = self.msgq.split_once('/').unwrap_or(("", &self.msgq));
        r.set_text("MsgqName", name);
        r.set_text("MsgqLib", lib);
        r.set_text("Text", &self.text);
        let mut out = r.buf;
        wm::obj::put_names(&mut out, &self.aggrscl);
        let mut extras = String::new();
        for (k, v) in &self.extras {
            let _ = write!(extras, "{k}={};", v.replace(';', " "));
        }
        let bytes = wm::rec::text_bytes(&extras, extras.chars().count());
        out.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
        out.extend_from_slice(&bytes[..bytes.len().min(usize::from(u16::MAX))]);
        out
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a line description".into()));
        }
        let u8_of = |f: &str| u8::try_from(r.uint(f)).unwrap_or(u8::MAX);
        let u16_of = |f: &str| u16::try_from(r.uint(f)).unwrap_or(u16::MAX);
        let msgq = {
            let (lib, name) = (r.text("MsgqLib"), r.text("MsgqName"));
            if lib.is_empty() {
                name
            } else {
                format!("{lib}/{name}")
            }
        };
        let cmnrcylmt = if r.uint("RcyCount") == RCY_SYSVAL {
            RcyLimit::SysVal
        } else {
            RcyLimit::Limit {
                count: u16_of("RcyCount"),
                interval: u16_of("RcyInterval"),
            }
        };
        let tail = r.tail();
        let (aggrscl, used) = wm::obj::get_names(tail)?;
        let rest = &tail[used..];
        let extras = if rest.len() >= 2 {
            let n = usize::from(u16::from_le_bytes([rest[0], rest[1]]));
            let text = wm::rec::text_of(rest.get(2..2 + n).unwrap_or(&[]));
            text.split(';')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        } else {
            BTreeMap::default()
        };
        Ok(Self {
            rsrcname: r.text("RsrcName"),
            bridge: r.text("Bridge"),
            online: r.flag("Online"),
            vrywait: u16_of("VryWait"),
            aggpcy: (r.text("AggStd"), r.text("AggPcy")),
            aggrscl,
            nws: (r.text("NwsName"), u16_of("NwsPort")),
            assocport: r.text("AssocPort"),
            adptadr: r.text("AdptAdr"),
            ethstd: r.text("EthStd"),
            linespeed: r.text("LineSpeed"),
            duplex: r.text("Duplex"),
            maxframe: u32::try_from(r.uint("MaxFrame")).unwrap_or(1496),
            maxctl: u16_of("MaxCtl"),
            exchid: r.text("ExchId"),
            threshold: r.text("Threshold"),
            gentstfrm: r.flag("GenTstFrm"),
            linkspeed: r.text("LinkSpeed"),
            costcnn: u8_of("CostCnn"),
            costbyte: u8_of("CostByte"),
            security: r.text("Security"),
            prpdly: r.text("PrpDly"),
            usrdfn: [u8_of("UsrDfn1"), u8_of("UsrDfn2"), u8_of("UsrDfn3")],
            autocrtctl: r.flag("AutoCrtCtl"),
            autodltctl: u32::try_from(r.uint("AutoDltCtl")).unwrap_or(AUTODLTCTL_NONE),
            cmnrcylmt,
            msgq,
            text: r.text("Text"),
            extras,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut l = Lind {
            rsrcname: "*AGG".into(),
            text: "Harness line".into(),
            ..Lind::default()
        };
        l.set("AGGRSCL", "CMN01 CMN02").unwrap();
        l.set("CMNRCYLMT", "(2 5)").unwrap();
        l.set("MSGQ", "QSYS/QSYSOPR").unwrap();
        l.set("AUTODLTCTL", "*NONE").unwrap();
        l.set("SRVOPT", "*NONE").unwrap();
        l.set("SSAP", "((04 1496 *CALC))").unwrap();
        let b = l.encode();
        assert_eq!(&b[..8], b"QSCMLIND");
        assert_eq!(Lind::decode(&b).unwrap(), l);
        let d = Lind::default();
        assert_eq!(Lind::decode(&d.encode()).unwrap(), d);
    }

    #[test]
    fn refuses_other_payloads() {
        assert!(Lind::decode(b"QSWMDEVD").is_err());
        let mut b = Lind::default().encode();
        b[0] = b'X';
        assert!(Lind::decode(&b).is_err());
    }
}
