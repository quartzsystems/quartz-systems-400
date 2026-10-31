// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update service commands (`docs/UPDATE_PROTOCOL.md`): `SNDPTFORD`,
//! `QSCHGUPDA`, `QSCHKUPD`, `SNDSRVRQS`, `WRKCNTINF` and `CHGCNTINF`.
//! The protocol is `ptf/update`; this is parameters, the save files in
//! `QGPL`, the attributes in data areas and the messages.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use cmd::block::Parms;
use ptf_update::deliver::{self, Sink};
use ptf_update::order::{OrderMode, OrderRequest};
use ptf_update::{CatalogPtf, Client};
use stg::proto::CmdReply;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::savf::{self, SavfAttr};
use store::obj::{ops, types};
use tls::rustls;

use super::direct::{failed, in_store};
use super::ptfcmd::{self, msg, release_text};
use super::savcmd::trusted_keys;
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "SNDPTFORD" => sndptford(system, parms),
        "QSCHGUPDA" => chgupda(system, parms),
        "QSCHKUPD" => chkupd(system).map(|(reply, _)| reply),
        "SNDSRVRQS" => sndsrvrqs(system, parms),
        "WRKCNTINF" => Ok(wrkcntinf(system)),
        "CHGCNTINF" => chgcntinf(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// The update attributes' data area in `QSYS`.
const ATTRS: &str = "QSUPDATTR";
/// The contact information's data area in `QSYS`.
const CONTACT: &str = "QSCNTINF";
/// The service a new system talks to.
pub const DEFAULT_URL: &str = "https://updates.quartz.systems/qs400";
/// An order delivered.
const ORDERED: &str = "CPZ8C38";
/// Updates are available (to `QSYSOPR`).
const AVAILABLE: &str = "CPI3E01";
/// Nothing newer.
const CURRENT: &str = "CPC3E02";
/// Attributes changed.
const CHANGED: &str = "CPC3E03";
/// A service request sent.
const SENT: &str = "CPC3E04";

/// `key=value` lines of a `*CHAR 2000` data area in `QSYS`.
fn kv_load(system: &mut System, name: &str) -> BTreeMap<String, String> {
    let r = in_store(system, |txn, qsys, _| {
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(name).expect("name");
        let Some(id) = lib.lookup(txn, types::DTAARA, n).map_err(|e| e.to_string())? else {
            return Ok(String::new());
        };
        Ok(match dtaara::read(txn, id).map_err(|e| e.to_string())? {
            DataAreaValue::Char(c) => c,
            _ => String::new(),
        })
    });
    r.unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim_end().to_string()))
        .collect()
}

fn kv_store(system: &mut System, name: &str, text: &str, kv: &BTreeMap<String, String>) -> Result<(), String> {
    let mut body = String::new();
    for (k, v) in kv {
        let _ = writeln!(body, "{k}={v}");
    }
    if body.len() > 2000 {
        return Err("CPD0084 the values do not fit".into());
    }
    in_store(system, |txn, qsys, _| {
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(name).expect("name");
        let v = DataAreaValue::Char(body);
        match lib.lookup(txn, types::DTAARA, n).map_err(|e| e.to_string())? {
            Some(id) => dtaara::write(txn, id, &v).map_err(|e| e.to_string()),
            None => dtaara::create(
                txn,
                &ops::CreateParams {
                    library: qsys,
                    name: n,
                    type_code: types::DTAARA,
                    attribute: String::new(),
                    text: text.into(),
                    owner: "QSYS".into(),
                    system: String::new(),
                    now: now_unix(),
                    by_system: true,
                },
                DataAreaType::Char { len: 2000 },
                &v,
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
        }
    })
}

fn attrs(system: &mut System) -> BTreeMap<String, String> {
    kv_load(system, ATTRS)
}

fn store_attrs(system: &mut System, kv: &BTreeMap<String, String>) -> Result<(), String> {
    kv_store(system, ATTRS, "Update service attributes", kv)
}

/// The TLS configuration for `https://`: the shipped roots.
fn tls_config() -> Option<Arc<rustls::ClientConfig>> {
    let mut roots = rustls::RootCertStore::empty();
    for r in certs::roots::roots() {
        let _ = roots.add(rustls::pki_types::CertificateDer::from(r.der.clone()));
    }
    tls::config::client_config(
        &tls::policy::TlsPolicy::opsys(),
        roots,
        None,
        tls::config::ServerAuthMode::Full,
    )
    .ok()
}

/// A client for the service the attributes name.
fn client(system: &mut System) -> Result<(Client, BTreeMap<String, String>), String> {
    let a = attrs(system);
    let url = a.get("url").map_or(DEFAULT_URL, String::as_str);
    let proxy = a.get("proxy").map_or("", String::as_str);
    let mut c = Client::new(url, proxy, tls_config()).map_err(|e| format!("{} {e}", e.msgid()))?;
    c.ident.system = system.system_name();
    Ok((c, a))
}

fn sequence(a: &BTreeMap<String, String>) -> u64 {
    a.get("sequence").and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// Save files `QGPL/QSnnnnn`, made or replaced.
struct QgplSink<'a> {
    system: &'a mut System,
}

impl Sink for QgplSink<'_> {
    fn put(&mut self, e: &CatalogPtf, bytes: &[u8]) -> ptf_update::Result<String> {
        let id = e.id.clone();
        in_store(self.system, |txn, qsys, _| {
            let l = ops::find_library(txn, qsys, qsname::Name::parse("QGPL").expect("name"))
                .map_err(|e| format!("CPF2110 QGPL: {e}"))?
                .id;
            let n = qsname::Name::parse(&id).map_err(|e| format!("CPD0074 {e}"))?;
            let lib = store::obj::library::Library::load(txn, l).map_err(|e| e.to_string())?;
            let sid = match lib.lookup(txn, types::SAVF, n).map_err(|e| e.to_string())? {
                Some(s) => s,
                None => savf::create(
                    txn,
                    &ops::CreateParams {
                        library: l,
                        name: n,
                        type_code: types::SAVF,
                        attribute: String::new(),
                        text: format!("PTF {id} from the update service"),
                        owner: "QSYS".into(),
                        system: String::new(),
                        now: now_unix(),
                        by_system: true,
                    },
                    SavfAttr::default(),
                )
                .map_err(|e| e.to_string())?,
            };
            savf::write(txn, sid, bytes).map_err(|e| e.to_string())
        })
        .map_err(ptf_update::Error::Protocol)?;
        Ok(format!("QGPL/{}", e.id))
    }
}

fn require() -> Result<(), String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::ALLOBJ, "order PTFs")
}

fn sndptford(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let rmt = parms.text("RMTCPNAME").to_ascii_uppercase();
    if !rmt.is_empty() && rmt != "*IBMSRV" {
        return Err(format!("CPF8C24 remote control point {rmt}: orders go to the update service (*IBMSRV)"));
    }
    if parms.text("DLVRYFMT") == "*IMAGE" {
        return Err("CPF9814 DLVRYFMT(*IMAGE): image catalogs are not available yet; use *SAVF".into());
    }
    let rel = release_text();
    let mut items = Vec::new();
    for v in parms.list("PTFID") {
        let cmd::Value::Elems(e) = v else { continue };
        let id = e
            .first()
            .and_then(|x| x.as_ref())
            .and_then(cmd::Value::text)
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase();
        let release = e.get(2).and_then(|x| x.as_ref()).and_then(cmd::Value::text).unwrap_or("").trim();
        if !release.is_empty() && release != "*ONLYRLS" && release != rel {
            return Err(format!("CPF3598 {id}: release {release}; this system is {rel}"));
        }
        items.push(match id.as_str() {
            "*HIPERGRP" => "*HIPER".to_string(),
            _ => id,
        });
    }
    if items.is_empty() {
        return Err("CPD0084 PTFID names nothing to order".into());
    }
    let cover_only = parms.text("PTFPART") == "*CVRLTR";
    let mut db = ptfcmd::load_db(system)?;
    let req = OrderRequest {
        release: rel,
        order: if parms.text("ORDER") == "*PTFID" {
            OrderMode::PtfId
        } else {
            OrderMode::Required
        },
        reorder: parms.text("REORDER") == "*YES",
        cover_only,
        items,
        have: deliver::have(&db),
    };
    let (c, mut a) = client(system)?;
    let d = {
        let mut sink = QgplSink { system };
        deliver::send_order(&c, &trusted_keys(), sequence(&a), &req, &mut sink)
            .map_err(|e| format!("{} {e}", e.msgid()))?
    };
    let now = now_unix();
    for (id, savf) in &d.delivered {
        deliver::record(&mut db, d.catalog.ptf(id).expect("checked"), savf, cover_only, now);
    }
    ptfcmd::store_db(system, &db)?;
    let mut groups = ptfcmd::load_groups(system)?;
    for (g, m) in &d.manifests {
        groups.put(g, m);
    }
    for (g, level) in &d.order.groups {
        groups.offer(g, *level);
    }
    ptfcmd::store_groups(system, &groups)?;
    a.insert("sequence".into(), d.catalog.sequence.max(sequence(&a)).to_string());
    store_attrs(system, &a)?;
    let mut reply = CmdReply::default();
    for (id, savf) in &d.delivered {
        let _ = writeln!(reply.text, "PTF {id} received as save file {savf}");
    }
    for (g, l) in &d.order.groups {
        let _ = writeln!(reply.text, "PTF group {g} level {l} received");
    }
    for m in &d.order.missing {
        let _ = writeln!(reply.text, "{m} is not known to the update service");
    }
    if d.delivered.is_empty() && d.order.groups.is_empty() {
        let _ = writeln!(reply.text, "Nothing to deliver: this system has what was ordered");
    }
    let n = d.delivered.len().to_string();
    let _ = writeln!(reply.text, "{ORDERED} order {} complete, {n} PTF(s)", d.order.order);
    reply.messages.push(msg(ORDERED, 4, &[(&d.order.order, 10), (&n, 5)]));
    Ok(reply)
}

fn chgupda(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::IOSYSCFG, "change the update attributes")?;
    let mut a = attrs(system);
    let url = parms.text("URL");
    match url.trim() {
        "" | "*SAME" => {}
        "*DFT" => {
            a.remove("url");
        }
        u => {
            ptf_update::Url::parse(u).map_err(|e| format!("CPD0084 URL: {e}"))?;
            a.insert("url".into(), u.into());
        }
    }
    match parms.text("PROXY").trim() {
        "" | "*SAME" => {}
        "*NONE" => {
            a.remove("proxy");
        }
        p => {
            ptf_update::url::host_port(p, 8080).ok_or_else(|| format!("CPD0084 PROXY({p}) is not host:port"))?;
            a.insert("proxy".into(), p.into());
        }
    }
    match parms.text("SCHEDULE").as_str() {
        "" | "*SAME" => {}
        s => {
            a.insert("schedule".into(), s.into());
        }
    }
    let t = parms.text("SCDTIME");
    let t = t.trim();
    if !t.is_empty() && t != "*SAME" {
        if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("CPD0084 SCDTIME({t}) is not a time"));
        }
        a.insert("time".into(), t.into());
    }
    store_attrs(system, &a)?;
    Ok(super::done(
        format!(
            "{CHANGED} update attributes changed: {} schedule {}\n",
            a.get("url").map_or(DEFAULT_URL, String::as_str),
            a.get("schedule").map_or("*NONE", String::as_str)
        ),
        CHANGED,
        &[],
    ))
}

/// `QSCHKUPD`: the catalog against the system; a message to `QSYSOPR`
/// when something is newer. Whether it was.
pub fn chkupd(system: &mut System) -> Result<(CmdReply, bool), String> {
    let (service, mut kv) = client(system)?;
    let cat = service
        .catalog(&trusted_keys(), sequence(&kv))
        .map_err(|e| format!("{} {e}", e.msgid()))?;
    let db = ptfcmd::load_db(system)?;
    let mut groups = ptfcmd::load_groups(system)?;
    let found = deliver::compare(&cat, &db, &mut groups, &release_text(), &trusted_keys());
    ptfcmd::store_groups(system, &groups)?;
    kv.insert("sequence".into(), cat.sequence.max(sequence(&kv)).to_string());
    kv.insert("last".into(), now_unix().to_string());
    store_attrs(system, &kv)?;
    let mut text = String::new();
    for (g, have, new) in &found.newer {
        let _ = writeln!(text, "PTF group {g}: level {new} available, {have} installed");
    }
    for p in &found.hiper {
        let _ = writeln!(text, "HIPER PTF {p} is not on this system");
    }
    if let Some(c) = &found.cumulative {
        let _ = writeln!(text, "Cumulative package {c} is available");
    }
    if !found.any() {
        return Ok((
            super::done(format!("{CURRENT} this system has the newest fixes the service offers\n"), CURRENT, &[]),
            false,
        ));
    }
    let count = (found.newer.len() + found.hiper.len() + usize::from(found.cumulative.is_some())).to_string();
    let summary = format!("Updates are available from the update service: {count} item(s). WRKPTFGRP shows the groups.");
    let m = wm::obj::msgq::QMsg {
        msgid: AVAILABLE.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 10,
        text: summary.clone(),
        data: cmd::error::data(&[(&count, 5)]),
        job: ("QSYSCOMM1".into(), "QSYS".into(), 0),
        from_pgm: "QSZUCHK".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = crate::stgsvc::msgs::send(system, "QSYS", "QSYSOPR", m);
    let _ = writeln!(text, "{AVAILABLE} {summary}");
    Ok((
        CmdReply {
            text,
            ..CmdReply::default()
        },
        true,
    ))
}

/// The hourly look at the schedule (`QSYSCOMM1`): run `QSCHKUPD` when
/// a daily or weekly check is due. Lines for the job log.
pub fn tick(system: &mut System, now: u64) -> Vec<String> {
    let a = attrs(system);
    let period = match a.get("schedule").map(String::as_str) {
        Some("*DAILY") => 86_400,
        Some("*WEEKLY") => 7 * 86_400,
        _ => return Vec::new(),
    };
    let last: u64 = a.get("last").and_then(|s| s.parse().ok()).unwrap_or(0);
    // Due at the scheduled time of day once the period has passed.
    let at = a
        .get("time")
        .and_then(|t| {
            let h: u64 = t.get(0..2)?.parse().ok()?;
            let m: u64 = t.get(2..4)?.parse().ok()?;
            Some(h * 3600 + m * 60)
        })
        .unwrap_or(0);
    // Due once the period (less the tick's hour) has passed and the time
    // of day (UTC) is reached; a check missed by more than an hour runs
    // at the next tick.
    let since = now.saturating_sub(last);
    let late = since >= period + 3600;
    if !late && (since + 3600 < period || (now % 86_400) < at) {
        return Vec::new();
    }
    match chkupd(system) {
        Ok((r, _)) => r.text.lines().map(str::to_string).collect(),
        Err(e) => {
            // Note the try so a service that is down is not asked hourly.
            let mut a = attrs(system);
            a.insert("last".into(), now.to_string());
            let _ = store_attrs(system, &a);
            vec![e]
        }
    }
}

fn contact(system: &mut System) -> String {
    let c = kv_load(system, CONTACT);
    c.get("contact").cloned().unwrap_or_default()
}

fn sndsrvrqs(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    if parms.text("ACTION") == "*REPORT" {
        return Err("CPF3700 problem reports to the update service arrive in a later release".into());
    }
    let who = contact(system);
    let (c, _) = client(system)?;
    let line = c.service("*TEST", &who).map_err(|e| format!("{} {e}", e.msgid()))?;
    let ticket = line.strip_prefix("ok ").unwrap_or(&line).trim().to_string();
    Ok(super::done(
        format!("{SENT} test request answered by {}: {ticket}\n", c.url),
        SENT,
        &[(&ticket, 10)],
    ))
}

/// The contact information's keys and labels.
const CONTACT_FIELDS: &[(&str, &str, &str)] = &[
    ("CMPNY", "company", "Company"),
    ("CONTACT", "contact", "Contact"),
    ("TELNBR", "telephone", "Telephone number"),
    ("EMAILADR", "email", "E-mail address"),
    ("MEDPTF", "media", "Media for mailing PTFs"),
];

const ADDRESS: &[(&str, &str)] = &[
    ("street", "Street address"),
    ("city", "City or locality"),
    ("state", "State or province"),
    ("country", "Country or region"),
    ("postal", "Postal code"),
];

fn wrkcntinf(system: &mut System) -> CmdReply {
    let c = kv_load(system, CONTACT);
    let mut text = String::from("Contact information\n");
    for (_, key, label) in CONTACT_FIELDS.iter().take(4) {
        let _ = writeln!(text, "  {label:<32} {}", c.get(*key).map_or("", String::as_str));
    }
    for (key, label) in ADDRESS {
        let _ = writeln!(text, "  {label:<32} {}", c.get(*key).map_or("", String::as_str));
    }
    let _ = writeln!(text, "  {:<32} {}", CONTACT_FIELDS[4].2, c.get("media").map_or("*AUTOMATIC", String::as_str));
    let a = attrs(system);
    let _ = writeln!(text, "  {:<32} {}", "Update service", a.get("url").map_or(DEFAULT_URL, String::as_str));
    CmdReply {
        text,
        ..CmdReply::default()
    }
}

fn chgcntinf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::IOSYSCFG, "change the contact information")?;
    let mut c = kv_load(system, CONTACT);
    for (kwd, key, _) in CONTACT_FIELDS {
        let v = parms.text(kwd);
        let v = v.trim();
        if !v.is_empty() && v != "*SAME" {
            c.insert((*key).into(), v.into());
        }
    }
    if let Some(cmd::Value::Elems(e)) = parms.get("MAILADDR") {
        for ((key, _), v) in ADDRESS.iter().zip(e) {
            if let Some(t) = v.as_ref().and_then(cmd::Value::text) {
                c.insert((*key).into(), t.trim().into());
            }
        }
    }
    kv_store(system, CONTACT, "Contact information", &c)?;
    Ok(super::done(format!("{CHANGED} contact information changed\n"), CHANGED, &[]))
}
