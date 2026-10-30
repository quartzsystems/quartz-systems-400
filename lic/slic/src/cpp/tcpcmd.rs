// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP configuration commands: interfaces, routes, the host
//! table, the domain, the attributes, port restrictions, servers and
//! the related tables, each changing the configuration through
//! `stgsvc::tcp::change` (the `QUSRSYS` files, then the kernel when
//! TCP/IP is active); `STRTCPIFC` and `ENDTCPIFC` through
//! `stgsvc::tcp::operate`. A change needs `*IOSYSCFG`.

use std::net::{IpAddr, Ipv4Addr};

use cmd::Value;
use cmd::block::Parms;
use stg::proto::{CmdMsg, CmdReply};
use tcpcfg::addr::{IfcAddr, parse_ip, parse_mask, parse_v4, unquote};
use tcpcfg::apply;
use tcpcfg::files::Table;
use tcpcfg::model::host::{HostEntry, NameChange};
use tcpcfg::model::ifc::Interface;
use tcpcfg::model::port::PortRestriction;
use tcpcfg::model::rte::{Dest, Route, RouteKey};
use tcpcfg::model::svr::UserServer;
use tcpcfg::model::tables::{NetworkEntry, ProtocolEntry, ServiceEntry, add_network, add_protocol, remove_named};
use tcpcfg::model::{LineRef, Proto, Tos, is_same, text_of, yes_no};
use tcpcfg::msg::{self, Note};
use tcpcfg::{Config, Error};

use crate::ipl::system::System;
use crate::stgsvc::tcp;

/// Run one of the commands; `None` when it is not one.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "ADDTCPIFC" => add_ifc(system, parms),
        "CHGTCPIFC" => chg_ifc(system, parms),
        "RMVTCPIFC" => rmv_ifc(system, parms),
        "STRTCPIFC" => str_ifc(system, parms),
        "ENDTCPIFC" => end_ifc(system, parms),
        "ADDTCPRTE" => add_rte(system, parms),
        "CHGTCPRTE" => chg_rte(system, parms),
        "RMVTCPRTE" => rmv_rte(system, parms),
        "ADDTCPHTE" => add_hte(system, parms),
        "CHGTCPHTE" => chg_hte(system, parms),
        "RMVTCPHTE" => rmv_hte(system, parms),
        "RNMTCPHTE" => rnm_hte(system, parms),
        "MRGTCPHT" => mrg_ht(system, parms),
        "CPYTCPHT" => cpy_ht(system, parms),
        "CHGTCPDMN" => chg_dmn(system, parms),
        "CHGTCPA" => chg_attrs(system, parms),
        "ADDTCPPORT" => add_port(system, parms),
        "RMVTCPPORT" => rmv_port(system, parms),
        "ADDTCPSVR" => add_svr(system, parms),
        "CHGTCPSVR" => chg_svr(system, parms),
        "RMVTCPSVR" => rmv_svr(system, parms),
        "ADDSRVTBLE" | "RMVSRVTBLE" | "ADDPCLTBLE" | "RMVPCLTBLE" | "ADDNETTBLE" | "RMVNETTBLE" => {
            tables(system, name, parms)
        }
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => refused(name, &e),
    })
}

/// The escape.
fn refused(name: &str, e: &Error) -> CmdReply {
    CmdReply {
        text: format!("{} {}\n", e.msgid, e.text),
        escape: Some(CmdMsg {
            msgid: e.msgid.clone(),
            kind: 7,
            data: cmd::error::data(&[(&e.text, 128), (name, 10)]),
        }),
        ..CmdReply::default()
    }
}

/// The completion message and the engine's notes after it.
fn completed(msgid: &str, what: &str, text: &str, notes: &[Note]) -> CmdReply {
    let mut out = format!("{msgid} {text}\n");
    for n in notes {
        out.push_str(&n.line());
        out.push('\n');
    }
    CmdReply {
        text: out,
        messages: vec![CmdMsg {
            msgid: msgid.into(),
            kind: 4,
            data: cmd::error::data(&[(what, 20)]),
        }],
        ..CmdReply::default()
    }
}

/// `*IOSYSCFG`, for a change.
fn need_iosyscfg(what: &str) -> Result<(), Error> {
    let Some(session) = sec::guard::current() else {
        return Ok(());
    };
    let p = session.borrow().principal.clone();
    p.require(sec::profile::SpecialAuth::IOSYSCFG, what)
        .map_err(|e| {
            let t = e.to_string();
            let id = super::leading_msgid(&t).unwrap_or("CPF2283").to_string();
            Error::new(&id, t.strip_prefix(&id).unwrap_or(&t).trim().to_string())
        })
}

/// A change to a table, with its completion message.
fn changed(
    system: &mut System,
    table: Table,
    msgid: &str,
    what: &str,
    f: impl FnOnce(&mut Config) -> tcpcfg::Result<()>,
) -> Result<CmdReply, Error> {
    need_iosyscfg(what)?;
    let (_, notes) = tcp::change(system, table, |c| f(c).map(|()| String::new()))?;
    let verb = match msgid {
        msg::ADDED => "added",
        msg::CHANGED => "changed",
        _ => "removed",
    };
    Ok(completed(msgid, what, &format!("{what} entry {verb}."), &notes))
}

// --- values

/// A value's text without quotes or trailing blanks.
fn raw(v: &Value) -> String {
    match v {
        Value::Char(s) => s.trim_end().to_string(),
        Value::Num(d) => d.to_string(),
        other => unquote(&other.source()).to_string(),
    }
}

fn opt_text(parms: &Parms, kwd: &str) -> Option<String> {
    parms.get(kwd).map(raw)
}

/// A number, `None` when absent or one of `skip` (a `*SAME`).
fn num(parms: &Parms, kwd: &str, skip: &[i64]) -> Option<i64> {
    parms.int(kwd).filter(|n| !skip.contains(n))
}

fn u_in<T: TryFrom<i64>>(n: i64, kwd: &str) -> Result<T, Error> {
    T::try_from(n).map_err(|_| tcpcfg::addr::bad(kwd, &n.to_string()))
}

/// A group's element `i`, as text.
fn elem(parms: &Parms, kwd: &str, i: usize) -> Option<String> {
    match parms.get(kwd)? {
        Value::Elems(items) => items.get(i)?.as_ref().map(raw),
        other if i == 0 => Some(raw(other)),
        _ => None,
    }
}

/// `LIND(name vlan)` or a single value.
fn line_of(parms: &Parms, kwd: &str) -> Result<Option<(LineRef, Option<u16>)>, Error> {
    let Some(first) = elem(parms, kwd, 0) else {
        return Ok(None);
    };
    if is_same(&first) || first.eq_ignore_ascii_case("*ADRALIAS") {
        return Ok(None);
    }
    let line = LineRef::parse(&first)?;
    let vlan = elem(parms, kwd, 1)
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .map(|v| u_in::<u16>(v, kwd))
        .transpose()?;
    Ok(Some((line, vlan)))
}

fn tos_of(parms: &Parms) -> Result<Option<Tos>, Error> {
    match parms.int("TOS") {
        Some(0) | None => Ok(None),
        Some(n) => Tos::parse(&n.to_string()).map(Some),
    }
}

fn mask_or_none(s: &str) -> Result<Option<Ipv4Addr>, Error> {
    let t = unquote(s);
    if t.is_empty() || t.eq_ignore_ascii_case("*NONE") || is_same(t) {
        Ok(None)
    } else {
        parse_mask(t).map(Some)
    }
}

fn ip_or_none(s: &str, kwd: &str) -> Result<Option<Ipv4Addr>, Error> {
    let t = unquote(s);
    if t.is_empty() || t.eq_ignore_ascii_case("*NONE") {
        Ok(None)
    } else {
        parse_v4(t).map(Some).map_err(|_| tcpcfg::addr::bad(kwd, t))
    }
}

fn text_param(parms: &Parms) -> Option<String> {
    opt_text(parms, "TEXT").filter(|t| !is_same(t)).map(|t| text_of(&t))
}

/// A list parameter's items as text.
fn items(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .into_iter()
        .map(|v| match v {
            Value::Elems(e) => e.first().and_then(Option::as_ref).map(raw).unwrap_or_default(),
            other => raw(other),
        })
        .collect()
}

// --- interfaces

fn lines_check(line: &LineRef) -> Result<(), Error> {
    let LineRef::Line(n) = line else {
        return Ok(());
    };
    let links = tcpcfg::net::NetOps::links(&mut tcpcfg::net::kernel::Kernel::new()).unwrap_or_default();
    let known = tcpcfg::line::KernelNames::new(links.into_iter().map(|l| l.name));
    if tcpcfg::line::LineState::exists(&known, n) {
        Ok(())
    } else {
        Err(Error::new(msg::LINE_NOT_FOUND, format!("Line {n} does not exist.")))
    }
}

fn add_ifc(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let addr = IfcAddr::parse(&parms.text("INTNETADR"))?;
    let (line, vlan) = line_of(parms, "LIND")?.ok_or_else(|| tcpcfg::addr::bad("LIND", ""))?;
    lines_check(&line)?;
    let mut i = Interface::new(addr, line);
    i.vlan = vlan;
    if let Some(m) = opt_text(parms, "SUBNETMASK") {
        i.mask = mask_or_none(&m)?;
    }
    if i.line == LineRef::Loopback && i.mask.is_none() && !addr.is_v6() {
        i.mask = Some(Ipv4Addr::new(255, 0, 0, 0));
    }
    if let Some(t) = tos_of(parms)? {
        i.tos = t;
    }
    i.mtu = num(parms, "MTU", &[0, -1]).map(|m| u_in(m, "MTU")).transpose()?;
    if let Some(a) = opt_text(parms, "AUTOSTART") {
        i.autostart = yes_no(&a, "AUTOSTART")?;
    }
    if let Some(l) = opt_text(parms, "LCLIFC") {
        i.local_ifc = ip_or_none(&l, "LCLIFC")?;
    }
    if let Some(a) = opt_text(parms, "ALIASNAME").filter(|a| !a.eq_ignore_ascii_case("*NONE") && !a.is_empty()) {
        i.alias = Some(a.to_ascii_uppercase());
    }
    if addr.is_v6() {
        i.prefix_len = num(parms, "ADRPFXLEN", &[]).map(|p| u_in(p, "ADRPFXLEN")).transpose()?;
        if let Some(d) = num(parms, "DADMAXTRN", &[]) {
            i.dad_max = u_in(d, "DADMAXTRN")?;
        }
        if let Some(p) = opt_text(parms, "PVYEXN") {
            i.privacy = yes_no(&p, "PVYEXN")?;
        }
        if let Some(g) = opt_text(parms, "IP6GENGLBA") {
            i.gen_global = yes_no(&g, "IP6GENGLBA")?;
        }
    }
    i.pref_ifc = items(parms, "PREFIFC").into_iter().filter(|p| !p.starts_with('*')).collect();
    if let Some(d) = opt_text(parms, "DHCPDYNDNS") {
        i.dhcp_dyndns = yes_no(&d, "DHCPDYNDNS")?;
    }
    if let Some(c) = opt_text(parms, "DHCPCLTID") {
        i.dhcp_client_id = c;
    }
    i.text = text_param(parms).unwrap_or_default();
    changed(system, Table::Ifc, msg::ADDED, "TCP/IP interface", |cfg| cfg.ifcs.add(i))
}

fn active(system: &mut System, key: &str) -> bool {
    tcp::with(system, |_, rt| rt.st.ifc_status(key).started()).unwrap_or(false)
}

fn chg_ifc(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let addr = IfcAddr::parse(&parms.text("INTNETADR"))?;
    let cfg = tcp::config(system).map_err(|e| Error::new("CPF9898", e))?;
    let mut i = cfg
        .ifcs
        .get(&addr)
        .cloned()
        .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("Interface {addr} is not configured.")))?;
    let running = active(system, &i.key());
    let line = line_of(parms, "LIND")?;
    let mask = opt_text(parms, "SUBNETMASK").filter(|m| !is_same(m));
    if running && (line.is_some() || mask.is_some()) {
        return Err(Error::new(
            msg::IN_USE,
            format!("The entry is in use: interface {addr} is active; end it to change its line or mask."),
        ));
    }
    if let Some((l, v)) = line {
        lines_check(&l)?;
        i.line = l;
        i.vlan = v;
    }
    if let Some(m) = mask {
        i.mask = mask_or_none(&m)?;
    }
    if let Some(t) = tos_of(parms)? {
        i.tos = t;
    }
    match parms.int("MTU") {
        Some(0) => i.mtu = None,
        Some(-1) | None => {}
        Some(m) => i.mtu = Some(u_in(m, "MTU")?),
    }
    if let Some(a) = opt_text(parms, "AUTOSTART").filter(|a| !is_same(a)) {
        i.autostart = yes_no(&a, "AUTOSTART")?;
    }
    if let Some(l) = opt_text(parms, "LCLIFC").filter(|l| !is_same(l)) {
        i.local_ifc = ip_or_none(&l, "LCLIFC")?;
    }
    if let Some(a) = opt_text(parms, "ALIASNAME").filter(|a| !is_same(a)) {
        i.alias = (!a.eq_ignore_ascii_case("*NONE") && !a.is_empty()).then(|| a.to_ascii_uppercase());
    }
    if let Some(p) = num(parms, "ADRPFXLEN", &[-1]) {
        i.prefix_len = Some(u_in(p, "ADRPFXLEN")?);
    }
    if let Some(d) = num(parms, "DADMAXTRN", &[-1]) {
        i.dad_max = u_in(d, "DADMAXTRN")?;
    }
    if let Some(p) = opt_text(parms, "PVYEXN").filter(|p| !is_same(p)) {
        i.privacy = yes_no(&p, "PVYEXN")?;
    }
    if let Some(g) = opt_text(parms, "IP6GENGLBA").filter(|g| !is_same(g)) {
        i.gen_global = yes_no(&g, "IP6GENGLBA")?;
    }
    let pref = items(parms, "PREFIFC");
    if !pref.iter().any(|p| is_same(p)) && parms.specified("PREFIFC") {
        i.pref_ifc = pref.into_iter().filter(|p| !p.starts_with('*')).collect();
    }
    if let Some(d) = opt_text(parms, "DHCPDYNDNS").filter(|d| !is_same(d)) {
        i.dhcp_dyndns = yes_no(&d, "DHCPDYNDNS")?;
    }
    if let Some(c) = opt_text(parms, "DHCPCLTID").filter(|c| !is_same(c)) {
        i.dhcp_client_id = c;
    }
    if let Some(t) = text_param(parms) {
        i.text = t;
    }
    changed(system, Table::Ifc, msg::CHANGED, "TCP/IP interface", |cfg| cfg.ifcs.change(i))
}

fn rmv_ifc(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let cfg = tcp::config(system).map_err(|e| Error::new("CPF9898", e))?;
    let addr = if let Some(a) = opt_text(parms, "INTNETADR").filter(|a| !a.is_empty()) {
        IfcAddr::parse(&a)?
    } else {
        let alias = parms.text("ALIASNAME");
        cfg.ifcs
            .by_alias(&alias)
            .map(|i| i.addr)
            .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("No interface has the alias name {alias}.")))?
    };
    if active(system, &addr.to_string()) {
        return Err(Error::new(
            msg::IN_USE,
            format!("The entry is in use: interface {addr} is active; end it first."),
        ));
    }
    changed(system, Table::Ifc, msg::REMOVED, "TCP/IP interface", |cfg| cfg.ifcs.remove(&addr).map(|_| ()))
}

fn str_ifc(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    need_iosyscfg("start a TCP/IP interface")?;
    let which = parms.text("INTNETADR");
    let notes = if which.eq_ignore_ascii_case("*AUTOSTART") {
        tcp::operate(system, |c| {
            let keys: Vec<String> = c
                .cfg
                .ifcs
                .list
                .iter()
                .filter(|i| i.autostart && !c.st.ifc_status(&i.key()).started())
                .map(Interface::key)
                .collect();
            let mut out = Vec::new();
            for k in keys {
                match apply::start_ifc(c, &k) {
                    Ok(n) => out.extend(n),
                    Err(e) => out.push(Note::diag(&e.msgid, e.text)),
                }
            }
            Ok(out)
        })?
    } else {
        tcp::operate(system, |c| apply::start_ifc(c, &which))?
    };
    Ok(notes_reply(&notes))
}

fn end_ifc(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    need_iosyscfg("end a TCP/IP interface")?;
    let which = parms.text("INTNETADR");
    let notes = tcp::operate(system, |c| apply::end_ifc(c, &which))?;
    Ok(notes_reply(&notes))
}

/// The engine's notes: the last information note is the completion.
fn notes_reply(notes: &[Note]) -> CmdReply {
    let text = notes.iter().fold(String::new(), |mut s, n| {
        s.push_str(&n.line());
        s.push('\n');
        s
    });
    let messages = notes
        .iter()
        .map(|n| CmdMsg {
            msgid: n.msgid.clone(),
            kind: if n.error { 5 } else { 4 },
            data: Vec::new(),
        })
        .collect();
    CmdReply {
        text,
        messages,
        ..CmdReply::default()
    }
}

// --- routes

/// The route key and prefix the parameters name.
fn route_key(parms: &Parms) -> Result<(Dest, u8, Tos, IpAddr), Error> {
    let dest = Dest::parse(&parms.text("RTEDEST"))?;
    let prefix = if dest.is_v6() {
        match parms.int("ADRPFXLEN") {
            Some(n) if n >= 0 => u_in(n, "ADRPFXLEN")?,
            _ => 64,
        }
    } else {
        match mask_or_none(&parms.text("SUBNETMASK"))? {
            Some(m) => tcpcfg::addr::mask_prefix(m).unwrap_or(32),
            None => 0,
        }
    };
    let prefix = if dest == Dest::DftRoute || dest.ip().is_unspecified() { 0 } else { prefix };
    let prefix = if dest == Dest::DftMcast { 4 } else { prefix };
    let tos = tos_of(parms)?.unwrap_or_default();
    let hop = parse_ip(&parms.text("NEXTHOP")).map_err(|_| tcpcfg::addr::bad("NEXTHOP", &parms.text("NEXTHOP")))?;
    Ok((dest, prefix, tos, hop))
}

fn add_rte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (dest, prefix, tos, hop) = route_key(parms)?;
    let mut r = Route::new(dest, prefix, hop);
    r.tos = tos;
    if let Some(b) = opt_text(parms, "BINDIFC") {
        r.bind_ifc = ip_or_none(&b, "BINDIFC")?;
    }
    if let Some(l) = line_of(parms, "BINDLIND")? {
        r.bind_line = Some((l.0.name().to_string(), l.1));
    }
    r.mtu = num(parms, "MTU", &[0, -1]).map(|m| u_in(m, "MTU")).transpose()?;
    if let Some(m) = num(parms, "METRIC", &[-1]) {
        r.metric = u_in(m, "METRIC")?;
    }
    if let Some(x) = opt_text(parms, "REDST") {
        r.redist = yes_no(&x, "REDST")?;
    }
    if let Some(d) = num(parms, "DUPRTEPTY", &[-1]) {
        r.dup_pty = u_in(d, "DUPRTEPTY")?;
    }
    r.text = text_param(parms).unwrap_or_default();
    changed(system, Table::Rte, msg::ADDED, "TCP/IP route", |cfg| cfg.rtes.add(r))
}

fn chg_rte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (dest, prefix, tos, hop) = route_key(parms)?;
    let key = RouteKey {
        dest,
        prefix,
        tos,
        next_hop: hop,
    };
    let cfg = tcp::config(system).map_err(|e| Error::new("CPF9898", e))?;
    let mut r = cfg
        .rtes
        .get(&key)
        .cloned()
        .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("No route to {dest} through {hop} is configured.")))?;
    if parms.specified("BINDIFC")
        && let Some(b) = opt_text(parms, "BINDIFC")
    {
        r.bind_ifc = ip_or_none(&b, "BINDIFC")?;
    }
    match parms.int("MTU") {
        Some(0) => r.mtu = None,
        Some(-1) | None => {}
        Some(m) => r.mtu = Some(u_in(m, "MTU")?),
    }
    if let Some(m) = num(parms, "METRIC", &[-1]) {
        r.metric = u_in(m, "METRIC")?;
    }
    if let Some(x) = opt_text(parms, "REDST").filter(|x| !is_same(x)) {
        r.redist = yes_no(&x, "REDST")?;
    }
    if let Some(d) = num(parms, "DUPRTEPTY", &[-1]) {
        r.dup_pty = u_in(d, "DUPRTEPTY")?;
    }
    if let Some(t) = text_param(parms) {
        r.text = t;
    }
    changed(system, Table::Rte, msg::CHANGED, "TCP/IP route", |cfg| cfg.rtes.change(r))
}

fn rmv_rte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (dest, prefix, tos, hop) = route_key(parms)?;
    let key = RouteKey {
        dest,
        prefix,
        tos,
        next_hop: hop,
    };
    changed(system, Table::Rte, msg::REMOVED, "TCP/IP route", |cfg| cfg.rtes.remove(&key).map(|_| ()))
}

// --- host table

fn host_addr(parms: &Parms, kwd: &str) -> Result<IpAddr, Error> {
    parse_ip(&parms.text(kwd)).map_err(|_| tcpcfg::addr::bad(kwd, &parms.text(kwd)))
}

fn add_hte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let addr = host_addr(parms, "INTNETADR")?;
    let names = items(parms, "HOSTNAME");
    let text = text_param(parms).unwrap_or_default();
    let e = HostEntry::new(addr, &names, &text)?;
    changed(system, Table::Host, msg::ADDED, "Host table", |cfg| cfg.hosts.add(e))
}

fn chg_hte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let addr = host_addr(parms, "INTNETADR")?;
    let given = items(parms, "HOSTNAME");
    let names: Option<Vec<NameChange>> = (parms.specified("HOSTNAME") && !given.iter().all(|n| is_same(n))).then(|| {
        given
            .iter()
            .map(|n| match n.to_ascii_uppercase().as_str() {
                "*SAME" | "" => NameChange::Same,
                "*BLANK" => NameChange::Blank,
                _ => NameChange::Name(n.clone()),
            })
            .collect()
    });
    let text = text_param(parms);
    changed(system, Table::Host, msg::CHANGED, "Host table", |cfg| {
        cfg.hosts.change(addr, names.as_deref(), text.as_deref())
    })
}

fn rmv_hte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let addr = host_addr(parms, "INTNETADR")?;
    changed(system, Table::Host, msg::REMOVED, "Host table", |cfg| cfg.hosts.remove(addr).map(|_| ()))
}

fn rnm_hte(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let from = host_addr(parms, "INTNETADR")?;
    let to = host_addr(parms, "NEWINTNETA")?;
    need_iosyscfg("rename a host table entry")?;
    let (_, notes) = tcp::change(system, Table::Host, |cfg| cfg.hosts.rename(from, to).map(|()| String::new()))?;
    let mut r = completed(msg::RENAMED, "Host table", &format!("Host table entry {from} renamed to {to}."), &notes);
    r.messages[0].data = cmd::error::data(&[(&from.to_string(), 45), (&to.to_string(), 45)]);
    Ok(r)
}

/// The records of a member of a database file through the job's names.
fn records(system: &mut System, lib: &str, file: &str, mbr: &str) -> Result<Vec<Vec<u8>>, Error> {
    let fail = |e: String| Error::new("CPF9812", e);
    super::direct::in_store(system, |txn, qsys, names| {
        let lib = match lib {
            "" | "*LIBL" => None,
            l => Some(qsname::Name::parse(l).map_err(|e| e.to_string())?),
        };
        let name = qsname::Name::parse(file).map_err(|e| e.to_string())?;
        let spec = match lib {
            Some(l) => qsname::LibSpec::Name(l),
            None => qsname::LibSpec::Libl,
        };
        let f = store::obj::ops::resolve(txn, qsys, names, spec, name, store::obj::types::FILE)
            .map_err(|_| format!("File {file} not found."))?;
        let ms = match mbr {
            "" | "*FIRST" => qsname::MemberSpec::First,
            "*LAST" => qsname::MemberSpec::Last,
            m => qsname::MemberSpec::Name(qsname::Name::parse(m).map_err(|e| e.to_string())?),
        };
        let m = db::Member::open_by(txn, f.id, ms).map_err(|e| e.to_string())?;
        Ok(m.scan(txn, 1, usize::MAX)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|s| s.record)
            .collect())
    })
    .map_err(fail)
}

fn mrg_ht(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    if parms.specified("FROMOBJ") {
        return Err(Error::new(
            msg::BAD_VALUE,
            "Value not valid: merging from a stream file (FROMOBJ) is not available in this release; use FROMFILE.",
        ));
    }
    let (file, lib) = parms.qual("FROMFILE");
    let mbr = parms.name("FROMMBR");
    let fmt = parms.name("FILEFMT");
    let replace = yes_no(&parms.text("REPLACE"), "REPLACE")?;
    let recs = records(system, &lib.to_ascii_uppercase(), &file.to_ascii_uppercase(), &mbr)?;
    let from = match fmt.as_str() {
        "*AIX" | "*NIC" => {
            let text = recs.iter().fold(String::new(), |mut s, r| {
                s.push_str(&ccsid::decode_trimmed(r));
                s.push('\n');
                s
            });
            if fmt == "*AIX" {
                tcpcfg::model::host::parse_aix(&text)
            } else {
                tcpcfg::model::host::parse_nic(&text)
            }
        }
        _ => {
            let l = &tcpcfg::files::defs::QATOCHOST;
            let mut t = Config::default();
            t.hosts.list.clear();
            let decoded = tcpcfg::files::decode(&mut |f| if f == l.file { recs.clone() } else { Vec::new() });
            decoded.hosts.list
        }
    };
    need_iosyscfg("merge the host table")?;
    let mut merged = tcpcfg::model::host::Merged::default();
    let (_, notes) = tcp::change(system, Table::Host, |cfg| {
        merged = cfg.hosts.merge(from, replace)?;
        Ok(String::new())
    })?;
    let text = format!("Host table merged: {} entries added, {} changed.", merged.added, merged.changed);
    let mut r = completed(msg::MERGED, "Host table", &text, &notes);
    let bin = |n: usize| i32::try_from(n).unwrap_or(i32::MAX).to_be_bytes();
    r.messages[0].data = [bin(merged.added), bin(merged.changed)].concat();
    Ok(r)
}

fn cpy_ht(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    if parms.specified("TOOBJ") {
        return Err(Error::new(
            msg::BAD_VALUE,
            "Value not valid: copying to a stream file (TOOBJ) is not available in this release; use TOFILE.",
        ));
    }
    let (file, lib) = parms.qual("TOFILE");
    let file = if file.is_empty() { "QTCPHT".to_string() } else { file.to_ascii_uppercase() };
    let lib = match lib.to_ascii_uppercase().as_str() {
        "" | "*LIBL" | "*CURLIB" => "QGPL".to_string(),
        l => l.to_string(),
    };
    let add = parms.name("MBROPT") == "*ADD";
    let cfg = tcp::config(system).map_err(|e| Error::new("CPF9898", e))?;
    let recs = tcpcfg::files::encode(&cfg, Table::Host)
        .into_iter()
        .find(|(f, _)| *f == tcpcfg::files::defs::QATOCHOST.file)
        .map(|(_, r)| r)
        .unwrap_or_default();
    let n = recs.len();
    crate::stgsvc::tcp::store::copy_to(system, &lib, &file, &recs, add).map_err(|e| Error::new("CPF9898", e))?;
    Ok(completed(
        msg::ADDED,
        "Host table",
        &format!("{n} host table records copied to {lib}/{file}."),
        &[],
    ))
}

// --- domain and attributes

fn chg_dmn(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let host = opt_text(parms, "HOSTNAME").filter(|h| !is_same(h));
    let dmn = opt_text(parms, "DMNNAME").filter(|d| !is_same(d));
    let sch = opt_text(parms, "DMNSCHLIST").filter(|d| !is_same(d));
    let pty = opt_text(parms, "HOSTSCHPTY").filter(|d| !is_same(d));
    let given = |k: &str| parms.specified(k);
    let dns: Vec<Option<String>> = (0..3)
        .map(|i| elem(parms, "INTNETADR", i).filter(|d| given("INTNETADR") && !is_same(d)))
        .collect();
    let port = num(parms, "PORT", &[0]);
    let proto = opt_text(parms, "PROTOCOL").filter(|d| !is_same(d));
    let inl = opt_text(parms, "INLDMNSVR").filter(|d| !is_same(d));
    let rty: Vec<Option<i64>> = (0..2)
        .map(|i| {
            elem(parms, "DMNSVRRTY", i)
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|n| *n > 0 && given("DMNSVRRTY"))
        })
        .collect();
    let sec = opt_text(parms, "DNSSEC").filter(|d| !is_same(d));
    changed(system, Table::TcpIp, msg::CHANGED, "TCP/IP domain", |cfg| {
        let d = &mut cfg.domain;
        if let Some(h) = host {
            d.host_name = h.to_ascii_uppercase();
        }
        if let Some(n) = dmn {
            d.domain_name = n.to_ascii_uppercase();
        }
        if let Some(s) = sch {
            d.search_list = s.split_whitespace().map(str::to_ascii_uppercase).collect();
        }
        if let Some(p) = pty {
            d.priority = if p == "*REMOTE" {
                tcpcfg::model::dmn::SearchPriority::Remote
            } else {
                tcpcfg::model::dmn::SearchPriority::Local
            };
        }
        if dns.iter().any(Option::is_some) {
            let mut list: Vec<IpAddr> = Vec::new();
            for (i, x) in dns.iter().enumerate() {
                let v = match x {
                    Some(v) => v.clone(),
                    None => d.dns.get(i).map(ToString::to_string).unwrap_or_default(),
                };
                if !v.is_empty() && !v.eq_ignore_ascii_case("*NONE") {
                    list.push(parse_ip(&v)?);
                }
            }
            d.dns = list;
        }
        if let Some(p) = port {
            d.port = u_in(p, "PORT")?;
        }
        if let Some(p) = proto {
            d.protocol = Proto::parse(&p)?;
        }
        if let Some(i) = inl {
            d.initial = if i == "*ROTATE" {
                tcpcfg::model::dmn::InitialServer::Rotate
            } else {
                tcpcfg::model::dmn::InitialServer::First
            };
        }
        if let Some(a) = rty[0] {
            d.retry_attempts = u_in(a, "DMNSVRRTY")?;
        }
        if let Some(b) = rty[1] {
            d.retry_interval = u_in(b, "DMNSVRRTY")?;
        }
        if let Some(s) = sec {
            d.dnssec = yes_no(&s, "DNSSEC")?;
        }
        d.validate()
    })
}

/// One `CHGTCPA` parameter's new value, `None` for `*SAME`. A group's
/// elements left `*SAME` keep the current ones.
fn attr_value(parms: &Parms, s: &tcpcfg::model::attr::ParamSpec, cur: &str) -> Option<String> {
    // A parameter left out keeps its value: a group's elements bind to
    // their own defaults, not to *SAME.
    if !parms.specified(s.kwd) {
        return None;
    }
    let v = parms.get(s.kwd)?;
    let same = |t: &str| is_same(t) || t == "-1" || t == "-1.0";
    match v {
        Value::Elems(items) => {
            let cur: Vec<&str> = cur.split_whitespace().collect();
            let elems: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, x)| {
                    let t = x.as_ref().map(raw).unwrap_or_default();
                    if same(&t) || t.is_empty() {
                        cur.get(i).copied().unwrap_or("").to_string()
                    } else if t == "0" && s.kwd == "NFC" && i == 1 {
                        "*NOMAX".into()
                    } else if t == "0" && s.kwd == "IPPATHMTU" && i == 1 {
                        "*ONCE".into()
                    } else {
                        t
                    }
                })
                .collect();
            Some(elems.join(" ").trim().to_string())
        }
        Value::List(list) => {
            let vals: Vec<String> = list
                .iter()
                .map(|x| match x {
                    Value::Elems(e) => e.iter().flatten().map(raw).collect::<Vec<_>>().join("/"),
                    other => raw(other),
                })
                .collect();
            if vals.iter().any(|x| same(x)) {
                None
            } else {
                Some(vals.join(" "))
            }
        }
        other => {
            let t = raw(other);
            (!same(&t)).then_some(t)
        }
    }
}

fn chg_attrs(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    need_iosyscfg("change TCP/IP attributes")?;
    let (_, notes) = tcp::change(system, Table::TcpIp, |cfg| {
        for s in tcpcfg::model::attr::SPEC {
            let cur = cfg.attrs.get(s.kwd);
            if let Some(v) = attr_value(parms, s, &cur) {
                let v = if s.kwd == "IP6TMPAVL" || s.kwd == "IP6TMPAPL" {
                    v.replace(',', ".")
                } else {
                    v
                };
                cfg.attrs.set(s.kwd, &v)?;
            }
        }
        Ok(String::new())
    })?;
    let mut r = completed(msg::ATTRS_CHANGED, "TCP/IP attributes", "TCP/IP attributes changed.", &notes);
    r.messages[0].data = Vec::new();
    Ok(r)
}

// --- port restrictions

fn port_parms(parms: &Parms) -> Result<(Option<u16>, Option<u16>, Proto, String), Error> {
    let lower = elem(parms, "PORT", 0).and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
    let upper = elem(parms, "PORT", 1).and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
    let lower = (lower >= 0).then(|| u_in::<u16>(lower, "PORT")).transpose()?;
    let upper = (upper >= 0).then(|| u_in::<u16>(upper, "PORT")).transpose()?;
    let proto = Proto::parse(&parms.text("PROTOCOL"))?;
    Ok((lower, upper, proto, parms.name("USRPRF")))
}

fn add_port(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (lower, upper, proto, user) = port_parms(parms)?;
    let r = PortRestriction {
        lower: lower.ok_or_else(|| tcpcfg::addr::bad("PORT", "*ALL"))?,
        upper,
        proto,
        user,
    };
    changed(system, Table::Port, msg::ADDED, "Port restriction", |cfg| cfg.ports.add(r))
}

fn rmv_port(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (lower, upper, proto, user) = port_parms(parms)?;
    changed(system, Table::Port, msg::REMOVED, "Port restriction", |cfg| {
        cfg.ports.remove(lower, upper, proto, &user).map(|_| ())
    })
}

// --- servers

fn add_svr(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let (pgm, lib) = parms.qual("PGM");
    let s = UserServer {
        special: parms.name("SVRSPCVAL"),
        pgm: (pgm.to_ascii_uppercase(), lib.to_ascii_uppercase()),
        name: parms.text("SVRNAME"),
        kind: parms.text("SVRTYPE"),
        autostart: yes_no(&parms.text("AUTOSTART"), "AUTOSTART")?,
        text: text_param(parms).unwrap_or_default(),
    };
    changed(system, Table::Servers, msg::ADDED, "TCP/IP server", |cfg| cfg.servers.add_user(s))
}

fn chg_svr(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let special = parms.name("SVRSPCVAL");
    let (pgm, lib) = parms.qual("PGM");
    let name = opt_text(parms, "SVRNAME").filter(|v| !is_same(v));
    let kind = opt_text(parms, "SVRTYPE").filter(|v| !is_same(v));
    let auto = opt_text(parms, "AUTOSTART").filter(|v| !is_same(v));
    let auto = auto.map(|a| yes_no(&a, "AUTOSTART")).transpose()?;
    let text = text_param(parms);
    changed(system, Table::Servers, msg::CHANGED, "TCP/IP server", |cfg| {
        cfg.servers.change_user(&special, |u| {
            if !pgm.is_empty() && !is_same(&pgm) {
                u.pgm = (pgm.to_ascii_uppercase(), lib.to_ascii_uppercase());
            }
            if let Some(n) = name {
                u.name = n;
            }
            if let Some(k) = kind {
                u.kind = k;
            }
            if let Some(a) = auto {
                u.autostart = a;
            }
            if let Some(t) = text {
                u.text = t;
            }
        })
    })
}

fn rmv_svr(system: &mut System, parms: &Parms) -> Result<CmdReply, Error> {
    let special = parms.name("SVRSPCVAL");
    changed(system, Table::Servers, msg::REMOVED, "TCP/IP server", |cfg| {
        cfg.servers.remove_user(&special).map(|_| ())
    })
}

// --- the related tables

fn tables(system: &mut System, name: &str, parms: &Parms) -> Result<CmdReply, Error> {
    let aliases: Vec<String> = items(parms, "ALIASES").into_iter().filter(|a| !a.starts_with('*')).collect();
    let text = text_param(parms).unwrap_or_default();
    match name {
        "ADDSRVTBLE" => {
            let e = ServiceEntry {
                name: parms.text("SERVICE"),
                port: u_in(parms.int("PORT").unwrap_or(0), "PORT")?,
                proto: parms.text("PROTOCOL"),
                aliases,
                text,
            };
            changed(system, Table::Services, msg::ADDED, "Service table", |cfg| cfg.services.add(e))
        }
        "RMVSRVTBLE" => {
            let (n, p, pr) = (parms.text("SERVICE"), parms.int("PORT").unwrap_or(0), parms.text("PROTOCOL"));
            let p: u16 = u_in(p, "PORT")?;
            changed(system, Table::Services, msg::REMOVED, "Service table", |cfg| {
                cfg.services.remove(&n, p, &pr).map(|_| ())
            })
        }
        "ADDPCLTBLE" => {
            let e = ProtocolEntry {
                name: parms.text("PROTOCOL"),
                number: u_in(parms.int("PCLNBR").unwrap_or(-1), "PCLNBR")?,
                aliases,
                text,
            };
            changed(system, Table::Protocols, msg::ADDED, "Protocol table", |cfg| add_protocol(&mut cfg.protocols, e))
        }
        "RMVPCLTBLE" => {
            let n = parms.text("PROTOCOL");
            changed(system, Table::Protocols, msg::REMOVED, "Protocol table", |cfg| {
                remove_named(&mut cfg.protocols, &n, |e| &e.name, "Protocol")
            })
        }
        "ADDNETTBLE" => {
            let e = NetworkEntry {
                name: parms.text("NETWORK"),
                addr: parse_v4(&parms.text("INTNETADR"))?,
                aliases,
                text,
            };
            changed(system, Table::Networks, msg::ADDED, "Network table", |cfg| add_network(&mut cfg.networks, e))
        }
        _ => {
            let n = parms.text("NETWORK");
            changed(system, Table::Networks, msg::REMOVED, "Network table", |cfg| {
                remove_named(&mut cfg.networks, &n, |e| &e.name, "Network")
            })
        }
    }
}
