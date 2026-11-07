// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*TIMZON`: a time zone description, in QSYS. The payload holds a
//! [`sysval::tz::Zone`] as `CRTTIMZON` and `CHGTIMZON` set it.

use sysval::tz::{DstRule, Edge, Zone};

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "TimeZoneDescription",
    size: Some(376),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Offset", 10, 2),
        field("StdAbbr", 12, 10),
        field("StdFull", 22, 50),
        field("StdMsg", 72, 7),
        field("DstAbbr", 79, 10),
        field("DstFull", 89, 50),
        field("DstMsg", 139, 7),
        field("MsgfName", 146, 10),
        field("MsgfLib", 156, 10),
        field("HasDst", 166, 1),
        field("StartMonth", 167, 1),
        field("StartDay", 168, 1),
        field("StartWeek", 169, 1),
        field("StartTime", 170, 4),
        field("EndMonth", 174, 1),
        field("EndDay", 175, 1),
        field("EndWeek", 176, 1),
        field("Reserved", 177, 1),
        field("EndTime", 178, 4),
        field("Shift", 182, 2),
        field("YearOffset", 184, 2),
        field("AltName", 186, 132),
        field("Text", 318, 50),
        field("Reserved2", 368, 8),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMTZON";
const VERSION: u64 = 1;

fn put_edge(r: &mut Rec<'_>, side: &str, e: Edge) {
    r.set_uint(&format!("{side}Month"), u64::from(e.month));
    r.set_uint(&format!("{side}Day"), u64::from(e.weekday));
    r.set_uint(&format!("{side}Week"), u64::from(e.week));
    r.set_uint(&format!("{side}Time"), u64::from(e.secs));
}

fn get_edge(r: &Rec<'_>, side: &str) -> Edge {
    let byte = |k: &str| u8::try_from(r.uint(&format!("{side}{k}"))).unwrap_or(0);
    Edge {
        month: byte("Month"),
        weekday: byte("Day"),
        week: byte("Week"),
        secs: u32::try_from(r.uint(&format!("{side}Time"))).unwrap_or(0),
    }
}

/// Two bytes, two's complement.
fn signed(v: u64) -> i16 {
    i16::from_le_bytes(u16::try_from(v & 0xFFFF).unwrap_or(0).to_le_bytes())
}

fn unsigned(v: i32) -> u64 {
    u64::from(u16::from_le_bytes(
        i16::try_from(v).unwrap_or(0).to_le_bytes(),
    ))
}

/// The payload of `z`.
#[must_use]
pub fn encode(z: &Zone) -> Vec<u8> {
    let mut r = Rec::new(&LAYOUT);
    r.set_bytes("Magic", MAGIC);
    r.set_uint("Version", VERSION);
    r.set_uint("Offset", unsigned(z.offset));
    r.set_text("StdAbbr", &z.std_abbr);
    r.set_text("StdFull", &z.std_full);
    r.set_text("StdMsg", &z.std_msg);
    r.set_text("DstAbbr", &z.dst_abbr);
    r.set_text("DstFull", &z.dst_full);
    r.set_text("DstMsg", &z.dst_msg);
    r.set_text("MsgfName", &z.msgf.0);
    r.set_text("MsgfLib", &z.msgf.1);
    if let Some(d) = &z.dst {
        r.set_flag("HasDst", true);
        put_edge(&mut r, "Start", d.start);
        put_edge(&mut r, "End", d.end);
        r.set_uint("Shift", u64::from(d.shift));
    }
    r.set_uint("YearOffset", unsigned(i32::from(z.year_offset)));
    r.set_text("AltName", &z.alt);
    r.set_text("Text", &z.text);
    r.buf
}

/// Read the payload of object `name` back.
pub fn decode(name: &str, b: &[u8]) -> Result<Zone> {
    let r = Rec::read(&LAYOUT, b)?;
    if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
        return Err(Error::Corrupt("not a time zone description".into()));
    }
    let dst = r.flag("HasDst").then(|| DstRule {
        start: get_edge(&r, "Start"),
        end: get_edge(&r, "End"),
        shift: u16::try_from(r.uint("Shift")).unwrap_or(60),
    });
    Ok(Zone {
        name: name.trim().to_string(),
        offset: i32::from(signed(r.uint("Offset"))),
        std_abbr: r.text("StdAbbr"),
        std_full: r.text("StdFull"),
        std_msg: r.text("StdMsg"),
        dst_abbr: r.text("DstAbbr"),
        dst_full: r.text("DstFull"),
        dst_msg: r.text("DstMsg"),
        msgf: (r.text("MsgfName"), r.text("MsgfLib")),
        dst,
        year_offset: signed(r.uint("YearOffset")),
        alt: r.text("AltName"),
        text: r.text("Text"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_zone_round_trips() {
        for z in sysval::tz::shipped() {
            assert_eq!(decode(&z.name, &encode(z)).unwrap(), *z, "{}", z.name);
        }
        let mut z = sysval::tz::find("QN0500EST3").unwrap();
        z.offset = -779;
        z.year_offset = -140;
        z.text = "Probe".into();
        assert_eq!(decode(&z.name, &encode(&z)).unwrap(), z);
        assert!(decode("X", &[0; 376]).is_err());
    }
}
