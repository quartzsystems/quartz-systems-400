// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications trace commands: `STRCMNTRC`, `ENDCMNTRC`,
//! `DLTCMNTRC`, `CHKCMNTRC` and `PRTCMNTRC` over the traces of
//! `stgsvc::cmntrc`. Each needs `*SERVICE` (`CPF39A8` otherwise).
//! `PRTCMNTRC` spools its listing to `QSYSPRT`.

use std::net::IpAddr;

use cmd::Value;
use cmd::block::Parms;
use nettools::cmntrc::report::{Code, Select};
use nettools::cmntrc::{CfgType, Direction, Header, Status};
use nettools::msg::{self, Kind, Msg};
use stg::proto::{CmdMsg, CmdReply};

use crate::ipl::system::System;
use crate::stgsvc::cmntrc as trc;

/// Run one of the commands; `None` when it is not one.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    if !matches!(name, "STRCMNTRC" | "ENDCMNTRC" | "DLTCMNTRC" | "CHKCMNTRC" | "PRTCMNTRC") {
        return None;
    }
    let obj = parms.name("CFGOBJ");
    let r = authorized().and_then(|()| match name {
        "STRCMNTRC" => start(parms, &obj).map(|m| vec![m]),
        "ENDCMNTRC" => cfgtype(parms).and_then(|t| trc::end(&obj, t)).map(|m| vec![m]),
        "DLTCMNTRC" => cfgtype(parms).and_then(|t| trc::delete(&obj, t)).map(|m| vec![m]),
        "CHKCMNTRC" => check(parms, &obj),
        _ => print(system, parms, &obj),
    });
    let ty = parms.name("CFGTYPE");
    Some(match r {
        Ok(msgs) => reply(&msgs, &obj, &ty),
        Err(m) => escape(&m, &obj, &ty),
    })
}

/// `*SERVICE`, as the service tools ask.
fn authorized() -> Result<(), Msg> {
    let Some(session) = sec::guard::current() else {
        return Ok(());
    };
    let p = session.borrow().principal.clone();
    p.require(sec::profile::SpecialAuth::SERVICE, "communications trace")
        .map_err(|_| Msg::new(msg::TRC_NOT_AUTH, Kind::Escape, "Using the communications trace needs *SERVICE special authority."))
}

fn cfgtype(parms: &Parms) -> Result<CfgType, Msg> {
    let t = parms.name("CFGTYPE");
    CfgType::parse(&t).ok_or_else(|| Msg::new(msg::TRC_CANNOT, Kind::Escape, format!("{t} is not a type that can be traced.")))
}

/// `MAXSTG`: `128K`, `1M`, `*MAX` (as bytes).
fn maxstg(text: &str) -> u64 {
    let t = text.trim().to_ascii_uppercase();
    match t.as_str() {
        "" | "*MIN" => 128 * 1024,
        "*MAX" => 1024 * 1024 * 1024,
        _ => {
            let (n, unit) = t.split_at(t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len()));
            let n: u64 = n.parse().unwrap_or(128);
            match unit {
                "M" => n * 1024 * 1024,
                "G" => n * 1024 * 1024 * 1024,
                _ => n * 1024,
            }
        }
    }
}

fn start(parms: &Parms, obj: &str) -> Result<Msg, Msg> {
    let ty = cfgtype(parms)?;
    let ifname = trc::link_of(obj, ty)?;
    // *CALC (0) and *MAX (-1) keep every byte.
    let usrdta = parms.name("USRDTA").parse::<u32>().ok().filter(|n| *n > 0);
    let h = Header {
        cfgobj: obj.to_string(),
        cfgtype: ty,
        ifname,
        protocol: "ETHERNET".into(),
        maxstg: maxstg(&parms.text("MAXSTG")),
        wrap: parms.name("TRCFULL") != "*STOPTRC",
        usrdta,
        dir: Direction::parse(&parms.name("DTADIR")).unwrap_or(Direction::Both),
        text: match parms.text("TEXT").as_str() {
            "*BLANK" => String::new(),
            t => t.to_string(),
        },
        started: 0,
        ended: None,
        status: Status::Active,
    };
    trc::start(h)
}

fn check(parms: &Parms, obj: &str) -> Result<Vec<Msg>, Msg> {
    let obj = (obj != "*ALL" && !obj.is_empty()).then_some(obj);
    let t = parms.name("CFGTYPE");
    let ty = if t == "*ALL" || t.is_empty() { None } else { Some(cfgtype(parms)?) };
    trc::check(obj, ty)
}

fn addr(v: Option<&Value>) -> Option<IpAddr> {
    let t = v?.source();
    let t = t.trim().trim_matches('\'');
    if t.eq_ignore_ascii_case("*ALL") { None } else { t.parse().ok() }
}

fn print(system: &mut System, parms: &Parms, obj: &str) -> Result<Vec<Msg>, Msg> {
    let ty = cfgtype(parms)?;
    let ips = parms.elems("TCPIPADR");
    let first = addr(ips.first().copied().flatten());
    let second = addr(ips.get(1).copied().flatten());
    let sel = Select {
        tcp_only: parms.name("FMTTCP") != "*NO",
        addrs: first.map(|a| (a, second)),
        // *ALL is 0.
        port: parms.name("SLTPORT").parse::<u16>().ok().filter(|p| *p > 0),
        broadcast: parms.name("FMTBCST") != "*NO",
        code: match parms.name("CODE").as_str() {
            "*EBCDIC" => Code::Ebcdic,
            "*ASCII" => Code::Ascii,
            _ => Code::Calc,
        },
    };
    let lines = trc::listing(system, obj, ty, &sel)?;
    let printed = crate::svc::security::print_as(system, "QSYSPRT", "CMNTRC", &lines, false)
        .map_err(|e| Msg::new(msg::TRC_FAILED, Kind::Escape, format!("The trace could not be printed: {e}")))?;
    let _ = printed;
    Ok(vec![Msg::new(
        msg::TRC_PRINTED,
        Kind::Comp,
        format!("Communications trace of {obj} type {} printed to QSYSPRT.", ty.name()),
    )])
}

fn kind_code(k: Kind) -> u8 {
    match k {
        Kind::Info => 1,
        Kind::Comp => 4,
        Kind::Diag => 5,
        Kind::Escape => 7,
    }
}

fn data(m: &Msg, obj: &str, ty: &str) -> Vec<u8> {
    match m.id {
        msg::TRC_STATUS => Vec::new(),
        _ => cmd::error::data(&[(obj, 10), (ty, 4)]),
    }
}

fn reply(msgs: &[Msg], obj: &str, ty: &str) -> CmdReply {
    let text = msgs.iter().fold(String::new(), |mut s, m| {
        s.push_str(&m.line());
        s.push('\n');
        s
    });
    CmdReply {
        text,
        messages: msgs
            .iter()
            .map(|m| CmdMsg {
                msgid: m.id.into(),
                kind: kind_code(m.kind),
                data: data(m, obj, ty),
            })
            .collect(),
        ..CmdReply::default()
    }
}

fn escape(m: &Msg, obj: &str, ty: &str) -> CmdReply {
    CmdReply {
        text: format!("{}\n", m.line()),
        escape: Some(CmdMsg {
            msgid: m.id.into(),
            kind: 7,
            data: data(m, obj, ty),
        }),
        ..CmdReply::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_sizes() {
        assert_eq!(maxstg("128K"), 131_072);
        assert_eq!(maxstg("2M"), 2 * 1_048_576);
        assert_eq!(maxstg("*MAX"), 1_073_741_824);
        assert_eq!(maxstg(""), 131_072);
    }
}
