// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small servers' commands and their clients, run in the job:
//! `CHGNTPA`, `CHGLPDA` and `CHGSMTPA` keep their attributes in the
//! TCP/IP server table (`servers::attrs`); `SNDTCPSPLF` and `LPR` read
//! a spooled file through the service and send it with `work/print`'s
//! LPR client; `SNDSMTPEMM` builds a MIME message (`net/mail`) and
//! hands it to the mail router, as the job's `QtmmSendMail` does.

use std::fmt::Write as _;
use std::sync::Arc;

use cmd::{Resolved, Value};
use ifs::client::{IfsClient, read_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::O_RDONLY;
use mail::smtp::{self, Connector, Envelope, NoTls, Relay, RustlsConnector, TlsMode};
use mail::{Address, Attachment, Body, Message};
use mi::Job;
use servers::attrs::lpd::LpdAttrs;
use servers::attrs::ntp::{self as ntpa, NtpAttrs};
use servers::attrs::smtp::{self as smtpa, SmtpAttrs, SmtpTls};
use servers::attrs::is_same;
use servers::msg;
use stg::{Client, Shared};
use stg::remote::RemoteIfs;
use stg::tcp::{TcpOp, TcpReply};
use wm::wtr::WtrDest;

use super::{Fail, parms_of};
use crate::console::host::Host;
use crate::work::netsvc;

/// A value as text: elements blank-separated, a single value of a
/// qualified name alone.
fn text_of(v: &Value) -> String {
    match v {
        Value::Char(s) => s.trim().to_string(),
        Value::Elems(items) => items
            .iter()
            .map(|i| i.as_ref().map_or_else(|| "*SAME".to_string(), text_of))
            .collect::<Vec<_>>()
            .join(" "),
        Value::List(items) => items.iter().map(text_of).collect::<Vec<_>>().join(" "),
        Value::Qual(parts) if parts.first().is_some_and(|p| p.source().starts_with('*')) => {
            parts[0].source().trim().to_string()
        }
        other => other.source().trim().to_string(),
    }
}

/// Whether every element of a value leaves its attribute alone.
fn all_same(v: &Value) -> bool {
    match v {
        Value::Elems(items) => items.iter().all(|i| i.as_ref().is_none_or(all_same)),
        other => is_same(&text_of(other)),
    }
}

fn bad(e: &str) -> Fail {
    Fail::new(msg::BAD_VALUE, format!("Value not valid: {e}"))
}

/// Store a server's attributes.
fn store(host: &Host, server: &str, name: &str, autostart: Option<bool>, bag: Vec<(String, String)>) -> Result<String, Fail> {
    let op = TcpOp::SetServer {
        server: server.into(),
        autostart,
        bag,
    };
    match host.link.borrow_mut().with(|l| l.tcp(op)) {
        TcpReply::Done { .. } => Ok(format!("{} {name} attributes changed.\n", msg::ATTRS_CHANGED)),
        TcpReply::Failed { note } => Err(Fail::new(&note.msgid, note.line())),
        other => Err(Fail::new("CPF9898", format!("unexpected answer {other:?}"))),
    }
}

fn yes_no(v: &str) -> Option<bool> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*YES" => Some(true),
        "*NO" => Some(false),
        _ => None,
    }
}

/// `CHGNTPA`.
fn chgntpa(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let mut a = NtpAttrs::from_bag(&netsvc::bag(&host.link, "*NTP"));
    for kwd in ntpa::KEYWORDS {
        let v = match *kwd {
            "RMTSYS" => parms.list(kwd).into_iter().map(text_of).collect::<Vec<_>>().join(" "),
            _ => match parms.get(kwd) {
                Some(v) => text_of(v),
                None => continue,
            },
        };
        a.set(kwd, &v).map_err(|e| bad(&e))?;
    }
    store(host, "*NTP", "SNTP", Some(a.autostarts()), a.to_bag())
}

/// `CHGLPDA`.
fn chglpda(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let mut a = LpdAttrs::from_bag(&netsvc::bag(&host.link, "*LPD"));
    if let Some(v) = parms.get("NBRSVR") {
        a.set("NBRSVR", &text_of(v)).map_err(|e| bad(&e))?;
    }
    store(host, "*LPD", "LPD", yes_no(&parms.name("AUTOSTART")), a.to_bag())
}

/// `CHGSMTPA`.
fn chgsmtpa(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let mut a = SmtpAttrs::from_bag(&netsvc::bag(&host.link, "*SMTP"));
    for kwd in smtpa::KEYWORDS {
        let Some(v) = parms.get(kwd) else { continue };
        if all_same(v) {
            continue;
        }
        a.set(kwd, &text_of(v)).map_err(|e| bad(&e))?;
    }
    store(host, "*SMTP", "SMTP", yes_no(&parms.name("AUTOSTART")), a.to_bag())
}

/// This system's host name for protocol lines: `HOSTNAME.DMNNAME`, or
/// the system name.
fn local_host(link: &Shared) -> String {
    if let TcpReply::HostConfig { cfg } = link.borrow_mut().with(|l| l.tcp(TcpOp::HostConfig))
        && !cfg.host_name.trim().is_empty()
    {
        let h = cfg.host_name.trim().to_ascii_lowercase();
        let d = cfg.dmn_name.trim().to_ascii_lowercase();
        return if d.is_empty() || h.contains('.') { h } else { format!("{h}.{d}") };
    }
    link.borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(|| "qs400".to_string(), |v| ccsid::from_ebcdic(&v.data).trim().to_ascii_lowercase())
}

/// `SNDTCPSPLF` and `LPR`.
fn sndtcpsplf(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let file = parms.name("FILE");
    let not_sent = |why: &str| Fail::new(msg::SPLF_NOT_SENT, format!("Spooled file {file} not sent: {why}."));
    let mut rmtsys = parms.text("RMTSYS");
    if rmtsys.eq_ignore_ascii_case("*INTNETADR") {
        rmtsys = parms.text("INTNETADR");
    }
    if rmtsys.trim().is_empty() {
        return Err(not_sent("no remote system"));
    }
    let prtq = parms.text("PRTQ");
    let job_name = parms.get("JOB").map_or_else(|| "*".to_string(), text_of);
    let splnbr = match parms.int("SPLNBR") {
        Some(0) | None => "*ONLY".to_string(),
        // *ANY: the file of that name, the newest when there are more.
        Some(-1 | -2) => "*LAST".to_string(),
        Some(n) => n.to_string(),
    };
    let (attrs, data) = host
        .link
        .borrow_mut()
        .with(|l| l.splf_fetch(file.clone(), job_name, splnbr))
        .map_err(|e: String| not_sent(&e))?;
    let desttype = match parms.name("DESTTYP").as_str() {
        "*AS400" => print::sink::lpr::DestType::Os400,
        "*PSF2" => print::sink::lpr::DestType::Psf2,
        _ => print::sink::lpr::DestType::Other,
    };
    let dest = WtrDest::Remote {
        host: rmtsys.trim().to_string(),
        queue: prtq.trim().to_string(),
        desttype: desttype.code(),
        destopt: parms.text("DESTOPT"),
        seppage: parms.name("SEPPAGE") != "*NO",
        transform: parms.name("TRANSFORM") != "*NO",
        mfrtypmdl: parms.name("MFRTYPMDL"),
        // BLOCKED(phase10): WSCST(name) is not read here; *WSCST
        // models print as plain text.
        wscst: Vec::new(),
        local_host: local_host(&host.link),
    };
    let target = crate::printmap::target_of(&dest).ok_or_else(|| not_sent("no destination"))?;
    crate::printmap::send_network(&target, &attrs, &dest, &data, Arc::new(print::sink::PlainConnector))
        .map_err(|(id, detail, _)| not_sent(&format!("{id} {detail}")))?;
    let mut out = format!(
        "{} Spooled file {} sent to queue {} on {}.\n",
        msg::SPLF_SENT,
        attrs.name,
        prtq.trim(),
        rmtsys.trim()
    );
    if parms.yes("DLTSPLF") {
        let line = format!("DLTSPLF FILE({}) JOB({}) SPLNBR({})", attrs.name, attrs.job_text(), attrs.number);
        let o = super::run(job, host, &line);
        if let Some(f) = o.escape {
            let _ = writeln!(out, "{} {}", f.msgid, f.text);
        }
    }
    Ok(out)
}

/// The job user's mail address: `user@host.domain`.
fn sender(host: &Host) -> String {
    let me = host.link.borrow_mut().with(stg::Client::whoami);
    format!("{}@{}", me.user.trim().to_ascii_lowercase(), local_host(&host.link))
}

/// The mail router `CHGSMTPA` names, and TLS for it.
fn relay(link: &Shared) -> Result<(Relay, Box<dyn Connector>), String> {
    let a = SmtpAttrs::from_bag(&netsvc::bag(link, "*SMTP"));
    let h = a
        .smart_host()
        .ok_or("no mail router: CHGSMTPA names neither FWDHUBSVR nor MAILROUTER")?;
    let mut r = Relay::new(&h.host, &local_host(link));
    r.port = h.port;
    r.addrs = netsvc::resolve(link, &h.host, h.port);
    if r.addrs.is_empty() {
        return Err(format!("{} does not resolve", h.host));
    }
    r.tls = match a.tls() {
        SmtpTls::Never => TlsMode::None,
        SmtpTls::Required => TlsMode::Required,
        SmtpTls::Implicit => TlsMode::Implicit,
        SmtpTls::Offered if h.port == smtp::TLS_PORT => TlsMode::Implicit,
        SmtpTls::Offered => TlsMode::Opportunistic,
    };
    r.auth = a.auth();
    let auth = relay_auth(r.tls);
    let tls: Box<dyn Connector> = match crate::work::tls::client_config_with(&mut link.borrow_mut(), auth) {
        Ok(cfg) => Box::new(RustlsConnector(cfg)),
        Err(e) => {
            eprintln!("qsjob: mail router TLS: {e}");
            Box::new(NoTls)
        }
    };
    Ok((r, tls))
}

/// How the mail router's certificate is checked: against the shipped
/// roots and the trusted CAs of `*SYSTEM`, by name, when TLS is
/// required (`STARTTLS` required or implicit TLS). Opportunistic
/// `STARTTLS` takes any certificate (RFC 7435): it is there against
/// listeners, and a refusal would fail a mail clear text would send.
fn relay_auth(mode: TlsMode) -> tls::config::ServerAuthMode {
    match mode {
        TlsMode::Required | TlsMode::Implicit => tls::config::ServerAuthMode::Full,
        TlsMode::None | TlsMode::Opportunistic => tls::config::ServerAuthMode::Passthru,
    }
}

/// Hand `message` to the mail router: the completion text.
fn deliver(link: &Shared, env: &Envelope, message: &[u8]) -> Result<String, String> {
    let (relay, tls) = relay(link)?;
    let receipt = smtp::send(&relay, env, message, tls.as_ref()).map_err(|e| e.to_string())?;
    let mut out = format!(
        "{} E-mail sent to {} of {} recipients.\n",
        msg::MAIL_SENT,
        receipt.accepted.len(),
        env.to.len()
    );
    for (r, code, text) in &receipt.refused {
        let _ = writeln!(out, "{r}: {code} {text}");
    }
    Ok(out)
}

/// A stream file's bytes.
fn read_file(link: &Shared, path: &str) -> Result<Vec<u8>, String> {
    let c = RemoteIfs::new(link.clone());
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).map_err(|e| format!("{path}: {}", e.text()))?;
    let r = read_all(&c, fd).map_err(|e| format!("{path}: {}", e.text()));
    let _ = c.close(fd);
    r
}

/// The media type `ATTACH` names, or one from the file's extension.
fn media(kind: &str, name: &str) -> String {
    match kind.trim().to_ascii_uppercase().as_str() {
        "" | "*OCTET" => mail::mime::content_type_for(name).to_string(),
        "*PLAIN" => "text/plain".into(),
        "*HTML" => "text/html".into(),
        "*XML" => "text/xml".into(),
        other => other.to_ascii_lowercase(),
    }
}

/// `SNDSMTPEMM`.
fn sndsmtpemm(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let fail = |why: &str| Fail::new(msg::MAIL_NOT_SENT, format!("E-mail not sent: {why}."));
    if parms.name("SMIME") != "*NONE" && !parms.name("SMIME").is_empty() {
        return Err(fail("S/MIME is not supported"));
    }
    let (mut to, mut cc, mut bcc) = (Vec::new(), Vec::new(), Vec::new());
    for item in parms.list("RCP") {
        let (addr, kind) = match item {
            Value::Elems(e) => (
                e.first().cloned().flatten().map(|v| text_of(&v)).unwrap_or_default(),
                e.get(1).cloned().flatten().map_or_else(|| "*PRI".into(), |v| text_of(&v)),
            ),
            other => (text_of(other), "*PRI".into()),
        };
        let a = Address::parse(&addr).map_err(|e| fail(&e.to_string()))?;
        match kind.to_ascii_uppercase().as_str() {
            "*CC" => cc.push(a),
            "*BCC" => bcc.push(a),
            _ => to.push(a),
        }
    }
    let note = parms.text("NOTE");
    let note = if note.eq_ignore_ascii_case("*NONE") { String::new() } else { note };
    let body = if parms.name("CONTENT") == "*HTML" {
        Body::Html(note)
    } else {
        Body::Plain(note)
    };
    let mut attachments = Vec::new();
    for item in parms.list("ATTACH") {
        let Value::Elems(e) = item else { continue };
        let path = e.first().cloned().flatten().map(|v| text_of(&v)).unwrap_or_default();
        if path.is_empty() || path.eq_ignore_ascii_case("*NONE") {
            continue;
        }
        let kind = e.get(1).cloned().flatten().map(|v| text_of(&v)).unwrap_or_default();
        let text = e.get(2).cloned().flatten().is_some_and(|v| text_of(&v).eq_ignore_ascii_case("*TXT"));
        let data = read_file(&host.link, &path).map_err(|e| fail(&e))?;
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        attachments.push(Attachment {
            content_type: media(&kind, &name),
            name,
            data,
            text,
        });
    }
    let now = ntp::clock::system_now() / 1_000_000_000;
    let m = Message {
        from: Address::mailbox(&sender(host)),
        to,
        cc,
        bcc,
        subject: parms.text("SUBJECT"),
        body,
        attachments,
        date: now,
        host: local_host(&host.link),
        unique: (u64::try_from(now).unwrap_or(0) << 20) ^ u64::from(std::process::id()),
    };
    let env = Envelope {
        from: m.from.mailbox.clone(),
        to: m.recipients(),
    };
    deliver(&host.link, &env, &m.render()).map_err(|e| fail(&e))
}

/// `QtmmSendMail` for this job: the stream file as it is, to the
/// recipients given.
pub fn qtmm(link: &Shared, req: &sysapi::mail::MailRequest) -> Result<(), String> {
    let message = read_file(link, &req.file)?;
    let env = Envelope {
        from: Address::parse(&req.originator).map_err(|e| e.to_string())?.mailbox,
        to: req
            .recipients
            .iter()
            .map(|(a, _)| Address::parse(a).map(|x| x.mailbox).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?,
    };
    deliver(link, &env, &message).map(|_| ())
}

/// The commands of this module; `None` for another command.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "CHGNTPA" => chgntpa(host, r),
        "CHGLPDA" => chglpda(host, r),
        "CHGSMTPA" => chgsmtpa(host, r),
        "SNDTCPSPLF" | "LPR" => sndtcpsplf(job, host, r),
        "SNDSMTPEMM" => sndsmtpemm(host, r),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_tls_checks_the_router() {
        use tls::config::ServerAuthMode;
        assert_eq!(relay_auth(TlsMode::Required), ServerAuthMode::Full);
        assert_eq!(relay_auth(TlsMode::Implicit), ServerAuthMode::Full);
        assert_eq!(relay_auth(TlsMode::Opportunistic), ServerAuthMode::Passthru);
    }
}
