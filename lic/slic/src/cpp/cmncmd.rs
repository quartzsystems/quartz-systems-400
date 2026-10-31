// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications commands in the service: `CRTLINETH`,
//! `CHGLINETH`, `DLTLIND`, `DSPLIND` and `WRKLIND` (their text form),
//! `VRYCFG` of lines, controllers and devices, and `WRKCFGSTS`'s text.
//! The work is `stgsvc::cmn`'s; here the parameter block becomes its
//! arguments and its notes become the reply's messages.

use std::fmt::Write as _;

use cmd::Value;
use cmd::block::Parms;
use cmncfg::{Kind, Lind, msgs};
use stg::proto::{CmdMsg, CmdReply};

use crate::ipl::system::System;
use crate::stgsvc::cmn;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "CRTLINETH" => crtlineth(system, parms),
        "CHGLINETH" => chglineth(system, parms),
        "DLTLIND" => dltlind(system, parms),
        "DSPLIND" => dsplind(system, parms),
        "WRKLIND" => wrklind(system, parms),
        "VRYCFG" => vrycfg(system, parms),
        "WRKCFGSTS" => wrkcfgsts(system, parms),
        _ => return None,
    })
}

/// The `CRTLINETH` keywords a line description keeps, in prompt order.
const KEYWORDS: &[&str] = &[
    "RSRCNAME",
    "BRIDGE",
    "ONLINE",
    "VRYWAIT",
    "AGGPCY",
    "AGGRSCL",
    "NETCTL",
    "NWS",
    "ASSOCPORT",
    "ADPTADR",
    "EXCHID",
    "ETHSTD",
    "LINESPEED",
    "DUPLEX",
    "SRVOPT",
    "MAXFRAME",
    "SSAP",
    "GRPADR",
    "MAXCTL",
    "COSTCNN",
    "COSTBYTE",
    "SECURITY",
    "PRPDLY",
    "USRDFN1",
    "USRDFN2",
    "USRDFN3",
    "AUTOCRTCTL",
    "AUTODLTCTL",
    "CMNRCYLMT",
    "MSGQ",
    "LINKSPEED",
    "THRESHOLD",
    "GENTSTFRM",
    "AUT",
    "TEXT",
];

/// A value as the line description's setter reads it: a scalar's
/// text, a group's elements blank-separated, a list's items, a
/// qualified name as `LIB/NAME`.
fn text_of(v: &Value) -> String {
    match v {
        Value::Char(s) => s.trim_end().to_string(),
        Value::List(items) => items
            .iter()
            .map(|i| match i {
                Value::Elems(_) => format!("({})", text_of(i)),
                other => text_of(other),
            })
            .collect::<Vec<_>>()
            .join(" "),
        Value::Elems(items) => items
            .iter()
            .map(|i| i.as_ref().map_or_else(String::new, text_of))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        other => other.source(),
    }
}

/// The parameter's value for the setter; `AUTODLTCTL`'s `*NONE` comes
/// through the block as 0.
fn setter_value(kwd: &str, v: &Value) -> String {
    let t = text_of(v);
    if kwd == "AUTODLTCTL" && t == "0" {
        return "*NONE".into();
    }
    t
}

/// An exchange identifier for `EXCHID(*SYSGEN)`: `056` and five hex
/// digits from the system and line names.
fn sysgen_exchid(system: &System, line: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in system.system_name().bytes().chain(line.bytes()) {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    format!("056{:05X}", h & 0xF_FFFF)
}

/// A message for the reply: identifier, type and the object it names.
fn msg(msgid: &str, kind: u8, obj: &str) -> CmdMsg {
    CmdMsg {
        msgid: msgid.into(),
        kind,
        data: cmd::error::data(&[(obj, 10)]),
    }
}

/// The reply of a command that did what `notes` say.
fn done(notes: &[cmn::Note]) -> CmdReply {
    let mut r = CmdReply::default();
    for (id, obj) in notes {
        let m = msg(id, 4, obj);
        let text = crate::stgsvc::hist::message_text(id, &m.data);
        let _ = writeln!(r.text, "{id} {text}");
        r.messages.push(m);
    }
    r
}

/// A refusal line (`AAAnnnn detail`) as the reply's escape, its
/// identifier with the object as data; a `CPD` diagnostic is followed
/// by `escape` when one is given.
fn refused(e: &str, obj: &str, escape: Option<&str>) -> CmdReply {
    let id = super::leading_msgid(e).unwrap_or("CPF9898").to_string();
    let mut r = CmdReply {
        text: format!("{e}\n"),
        ..CmdReply::default()
    };
    match escape {
        Some(esc) if id.starts_with("CPD") => {
            r.messages.push(msg(&id, 5, obj));
            r.escape = Some(msg(esc, 7, obj));
        }
        _ => r.escape = Some(msg(&id, 7, obj)),
    }
    r
}

fn crtlineth(system: &mut System, parms: &Parms) -> CmdReply {
    let name = parms.name("LIND");
    let mut l = Lind::default();
    for kwd in KEYWORDS {
        if let Some(v) = parms.get(kwd)
            && let Err(e) = l.set(kwd, &setter_value(kwd, v))
        {
            return refused(&e, &name, None);
        }
    }
    if l.exchid == "*SYSGEN" {
        l.exchid = sysgen_exchid(system, &name);
    }
    if l.is_aggregate() && l.aggrscl.is_empty() {
        return refused("CPD0084 RSRCNAME(*AGG) needs AGGRSCL", &name, None);
    }
    match cmn::create_line(system, &name, &l) {
        Ok(()) => done(&[(msgs::LINE_CREATED, name)]),
        Err(e) => refused(&e, &name, None),
    }
}

fn chglineth(system: &mut System, parms: &Parms) -> CmdReply {
    let name = parms.name("LIND");
    let given: Vec<&str> = KEYWORDS
        .iter()
        .copied()
        .filter(|k| parms.specified(k))
        .collect();
    let r = cmn::change_line(system, &name, &given, |l| {
        for kwd in &given {
            if let Some(v) = parms.get(kwd) {
                l.set(kwd, &setter_value(kwd, v))?;
            }
        }
        if l.is_aggregate() && l.aggrscl.is_empty() {
            return Err("CPD0084 RSRCNAME(*AGG) needs AGGRSCL".into());
        }
        Ok(())
    });
    match r {
        Ok(()) => done(&[(msgs::LINE_CHANGED, name)]),
        Err(e) => refused(&e, &name, Some(msgs::LINE_NOT_CHANGED)),
    }
}

/// The names a generic parameter selects among `all`: `AB*`, `*ALL`,
/// or the name itself.
fn select(which: &str, all: Vec<String>) -> Vec<String> {
    match which.strip_suffix('*') {
        Some(p) => all.into_iter().filter(|n| n.starts_with(p)).collect(),
        None if which == "*ALL" => all,
        None => vec![which.to_string()],
    }
}

fn dltlind(system: &mut System, parms: &Parms) -> CmdReply {
    let which = parms.name("LIND");
    let names = match cmn::line_names(system, &|_| true) {
        Ok(all) => select(&which, all),
        Err(e) => return refused(&e, &which, None),
    };
    if names.is_empty() {
        return refused(
            &format!("{} no line description is {which}", msgs::LINE_NOT_FOUND),
            &which,
            None,
        );
    }
    let mut notes = Vec::new();
    for n in &names {
        match cmn::delete_line(system, n) {
            Ok(mut more) => notes.append(&mut more),
            Err(e) => {
                let mut r = refused(&e, n, None);
                let d = done(&notes);
                r.text = d.text + &r.text;
                r.messages = d.messages;
                return r;
            }
        }
    }
    done(&notes)
}

fn dsplind(system: &mut System, parms: &Parms) -> CmdReply {
    let name = parms.name("LIND");
    match crate::stgsvc::cmn::read_and_show(system, &name) {
        Ok(text) => CmdReply {
            text,
            ..CmdReply::default()
        },
        Err(e) => refused(&e, &name, None),
    }
}

fn wrklind(system: &mut System, parms: &Parms) -> CmdReply {
    let which = parms.name("LIND");
    let wanted = |n: &str| match which.strip_suffix('*') {
        Some(p) => n.starts_with(p),
        None => which.is_empty() || which.starts_with('*') || n == which,
    };
    match cmn::line_names(system, &wanted) {
        Ok(names) => {
            let mut text = String::from("Line        Type        Status\n");
            for n in names {
                let st = cmn::cfg_status(system, &n, "*LIN")
                    .map(|(_, t)| t)
                    .unwrap_or_default();
                let _ = writeln!(text, "{n:<10}  *ELAN       {st}");
            }
            CmdReply {
                text,
                ..CmdReply::default()
            }
        }
        Err(e) => refused(&e, &which, None),
    }
}

fn vrycfg(system: &mut System, parms: &Parms) -> CmdReply {
    let ty = parms.name("CFGTYPE");
    let on = match parms.name("STATUS").as_str() {
        "*ON" => true,
        "*OFF" => false,
        s => {
            return refused(
                &format!("CPF9B70 STATUS({s}) is not available in this release; *ON and *OFF are"),
                &ty,
                Some(msgs::VARY_NOT_PROCESSED),
            );
        }
    };
    let range_net = parms.name("RANGE") != "*OBJ";
    let vrywait = match parms.int("VRYWAIT") {
        Some(n) if n >= 0 => u16::try_from(n).ok(),
        _ => None,
    };
    let objs: Vec<String> = parms
        .list("CFGOBJ")
        .iter()
        .map(|v| text_of(v).to_ascii_uppercase())
        .collect();
    let Some(kind) = Kind::parse(&ty) else {
        return refused(
            &format!("CPD0084 CFGTYPE({ty}): lines, controllers and devices vary here"),
            &ty,
            Some(msgs::VARY_NOT_PROCESSED),
        );
    };
    let mut notes = Vec::new();
    let mut errors = Vec::new();
    for want in &objs {
        let names = if want.ends_with('*') {
            let all = cmn::cfgsts_names(system, kind);
            select(want, all)
        } else {
            vec![want.clone()]
        };
        for n in names {
            let r = match kind {
                Kind::Line => cmn::vary_line(system, &n, on, range_net, vrywait),
                Kind::Ctl => cmn::vary_ctl(system, &n, on, range_net),
                Kind::Dev => cmn::vary_dev(system, &n, on),
            };
            match r {
                Ok(mut more) => notes.append(&mut more),
                Err(e) => errors.push((e, n)),
            }
        }
    }
    if kind == Kind::Line && !on {
        // The interfaces on a line varied off end with it.
        crate::stgsvc::tcp::follow_lines(system);
    }
    let mut r = done(&notes);
    if !errors.is_empty() {
        for (e, n) in &errors {
            r.text.push_str(e);
            r.text.push('\n');
            let id = super::leading_msgid(e).unwrap_or("CPF9898");
            r.messages.push(msg(id, 5, n));
        }
        let first = &errors[0].1;
        r.escape = Some(msg(msgs::VARY_NOT_PROCESSED, 7, first));
    }
    r
}

fn wrkcfgsts(system: &mut System, parms: &Parms) -> CmdReply {
    let ty = parms.name("CFGTYPE");
    let Some(kind) = Kind::parse(&ty) else {
        return refused(
            &format!("CPD0084 CFGTYPE({ty}): lines, controllers and devices are shown here"),
            &ty,
            None,
        );
    };
    let cfgd = parms.name("CFGD");
    let range_net = parms.name("RANGE") != "*OBJ";
    let filter = parms.name("STATUS");
    match cmn::cfgsts_rows(system, kind, &cfgd, range_net, &filter) {
        Ok(rows) => CmdReply {
            text: cmn::cfgsts_text(&rows),
            ..CmdReply::default()
        },
        Err(e) => refused(&e, &ty, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_values_read_as_the_setter_wants() {
        let elems = Value::Elems(vec![
            Some(Value::Char("*LNKAGG".into())),
            Some(Value::Char("*SRCDESTP".into())),
        ]);
        assert_eq!(text_of(&elems), "*LNKAGG *SRCDESTP");
        let list = Value::List(vec![Value::Char("CMN01".into()), Value::Char("CMN02".into())]);
        assert_eq!(text_of(&list), "CMN01 CMN02");
        let ssap = Value::List(vec![Value::Elems(vec![
            Some(Value::Char("04".into())),
            None,
            Some(Value::Char("*CALC".into())),
        ])]);
        assert_eq!(text_of(&ssap), "(04 *CALC)");
        let zero = Value::Num(qdecimal::Decimal::new(0, 0).unwrap());
        assert_eq!(setter_value("AUTODLTCTL", &zero), "*NONE");
        assert_eq!(setter_value("VRYWAIT", &zero), "0");
    }

    #[test]
    fn generic_names_select() {
        let all = || vec!["ETHA".to_string(), "ETHB".into(), "LAN".into()];
        assert_eq!(select("ETH*", all()), ["ETHA", "ETHB"]);
        assert_eq!(select("*ALL", all()).len(), 3);
        assert_eq!(select("LAN", all()), ["LAN"]);
    }
}
