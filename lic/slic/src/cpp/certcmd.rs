// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate commands, Quartz extensions (IBM i manages
//! certificates only through DCM's web pages): `QSCRTCRTS`,
//! `QSDLTCRTS`, `QSCRTCRT`, `QSIMPCRT`, `QSEXPCRT`, `QSCHGCRT`,
//! `QSDLTCRT`, `QSWRKCRT` (text), `QSASNCRT`, `QSADDAPPID`,
//! `QSCHGAPPID`, `QSRMVAPPID`, `QSWRKAPPID` (text) and `QSCHKCRT`. The
//! work is `certs::admin`'s over the store files `stgsvc::certs`
//! stages; stream files named by `FROMSTMF`, `TOSTMF` and `CSR` are read
//! and written as the job. Every command needs `*ALLOBJ` and `*SECADM`.

use std::fmt::Write as _;

use certs::admin::appids::{self, client_auth_from, client_auth_name};
use certs::admin::entries::{self, Change, ExportFormat, ImportFormat, Row, Signer};
use certs::admin::{Ctx, Keystore, Note, keystores, msgs, refusal};
use certs::apps::{AppDef, AppKind, CaTrust};
use certs::crypto::keys::KeyAlgorithm;
use certs::x509::build::{CertKind, CertSpec, Subject};
use certs::x509::iso_date;
use cmd::Value;
use cmd::block::Parms;
use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_RDONLY, O_TRUNC, O_WRONLY};
use stg::proto::{CmdMsg, CmdReply, Sys2Data};

use crate::ipl::system::System;
use crate::stgsvc::certs::{expiry_scan, may_manage, with_stores};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "QSCRTCRTS" => crtcrts(system, parms),
        "QSDLTCRTS" => dltcrts(system, parms),
        "QSCRTCRT" => crtcrt(system, parms),
        "QSIMPCRT" => impcrt(system, parms),
        "QSEXPCRT" => expcrt(system, parms),
        "QSCHGCRT" => chgcrt(system, parms),
        "QSDLTCRT" => dltcrt(system, parms),
        "QSWRKCRT" => wrkcrt(system, parms),
        "QSASNCRT" => asncrt(system, parms),
        "QSADDAPPID" => addappid(system, parms),
        "QSCHGAPPID" => chgappid(system, parms),
        "QSRMVAPPID" => rmvappid(system, parms),
        "QSWRKAPPID" => wrkappid(system, parms),
        "QSCHKCRT" => chkcrt(system),
        _ => return None,
    };
    Some(r.unwrap_or_else(|n| reply(&[], Some(&n))))
}

/// The lengths of each message's variables (`QTCPMSG`).
fn lengths(msgid: &str) -> &'static [usize] {
    match msgid {
        "TCP1C02" | "TCP1C03" | "TCP1C04" | "TCP1C05" | "TCP1C06" | "TCP1C21" => &[64, 256],
        "TCP1C07" | "TCP1C13" | "TCP1C14" => &[64],
        "TCP1C08" => &[64, 100],
        "TCP1C09" | "TCP1C0A" | "TCP1C0B" | "TCP1C0C" | "TCP1C15" | "TCP1C16" => &[100],
        "TCP1C0D" => &[10, 10],
        "TCP1C19" => &[10],
        "TCP1C20" => &[64, 256, 10],
        _ => &[256],
    }
}

/// A message's data from its variables' values.
pub fn data(msgid: &str, values: &[String]) -> Vec<u8> {
    let l = lengths(msgid);
    let fields: Vec<(&str, usize)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (v.as_str(), l.get(i).copied().unwrap_or(256)))
        .collect();
    cmd::error::data(&fields)
}

/// A message's text with its data.
pub fn text(msgid: &str, data: &[u8]) -> String {
    use msgf::MsgLookup as _;
    thread_local! {
        static B: std::cell::RefCell<msgf::BuiltinMsgs> = std::cell::RefCell::new(msgf::BuiltinMsgs::new());
    }
    B.with(|b| {
        b.borrow_mut()
            .describe(msgid, "QTCPMSG", "QSYS")
            .map_or_else(|| wm::rec::text_of(data), |d| d.format(data))
    })
}

fn cmd_msg(n: &Note, kind: u8) -> CmdMsg {
    CmdMsg {
        msgid: n.msgid.into(),
        kind,
        data: data(n.msgid, &n.data),
    }
}

/// The reply: completions, then the escape when there is one.
fn reply(done: &[Note], escape: Option<&Note>) -> CmdReply {
    let mut r = CmdReply::default();
    for n in done {
        let m = cmd_msg(n, 4);
        let _ = writeln!(r.text, "{} {}", n.msgid, text(n.msgid, &m.data));
        r.messages.push(m);
    }
    if let Some(n) = escape {
        let m = cmd_msg(n, 7);
        let _ = writeln!(r.text, "{} {}", n.msgid, text(n.msgid, &m.data));
        r.escape = Some(m);
    }
    r
}

type Out = Result<CmdReply, Note>;

/// The refusal note of a service failure line (`AAAnnnn text`).
fn failed(line: &str) -> Note {
    let id = super::leading_msgid(line).unwrap_or(msgs::FAILED);
    let id: &'static str = match id {
        "TCP1C19" => msgs::NOT_AUTHORIZED,
        _ => msgs::FAILED,
    };
    let rest = line.get(8..).unwrap_or(line).trim();
    Note::new(id, &[rest])
}

fn check_authority() -> Result<(), Note> {
    let actor = crate::stgsvc::security::actor();
    may_manage(actor.as_ref()).map_err(|user| Note::new(msgs::NOT_AUTHORIZED, &[&user]))
}

/// The store a command names, with its password.
fn keystore(p: &Parms) -> Keystore {
    let name = p.text("KEYSTORE");
    let name = if name.is_empty() { "*SYSTEM".to_string() } else { name };
    Keystore::new(&name, &p.text("PASSWORD"))
}

/// Run `f` on the stores (written back when `write`), its error as the
/// command's escape.
fn on<T>(
    system: &mut System,
    ks: &Keystore,
    write: bool,
    f: impl FnOnce(&Ctx<'_>) -> certs::Result<T>,
) -> Result<T, Note> {
    let path = ks.path.clone();
    with_stores(system, &[path.as_str()], write, f)
        .map_err(|e| failed(&e))?
        .map_err(|e| refusal(&e, ks.shown()))
}

/// A value, `None` for a special value that means none or same.
fn opt(p: &Parms, kwd: &str) -> Option<String> {
    let t = p.text(kwd);
    let u = t.to_ascii_uppercase();
    (!t.is_empty() && !matches!(u.as_str(), "*NONE" | "*SAME" | "*STASH" | "*GEN" | "*CN")).then_some(t)
}

fn algorithm(p: &Parms) -> Result<KeyAlgorithm, Note> {
    let a = p.name("KEYALG");
    if a.is_empty() {
        return Ok(KeyAlgorithm::EcdsaP256);
    }
    KeyAlgorithm::from_name(&a).ok_or_else(|| Note::new(msgs::FAILED, &[&format!("key algorithm {a} is not one this system makes")]))
}

fn days(p: &Parms, dft: u32) -> u32 {
    p.int("VALIDITY")
        .and_then(|d| u32::try_from(d).ok())
        .filter(|d| *d > 0)
        .unwrap_or(dft)
}

fn subject(p: &Parms, cn: &str) -> Subject {
    Subject {
        common_name: cn.to_string(),
        organization: opt(p, "ORG"),
        organizational_unit: opt(p, "ORGUNIT"),
        locality: opt(p, "LOCALITY"),
        state: opt(p, "STATE"),
        country: opt(p, "COUNTRY").map(|c| c.to_ascii_uppercase()),
    }
}

fn crtcrts(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let stash = p.name("STASH") != "*NO";
    let password = ks.password.clone().unwrap_or_default();
    let notes = if ks.path == certs::paths::LOCAL_CA_STORE {
        let cn = opt(p, "CN").unwrap_or_else(|| format!("{} Local Certificate Authority", system.system_name()));
        let subj = subject(p, &cn);
        let alg = algorithm(p)?;
        let d = days(p, 3650);
        on(system, &ks, true, |ctx| keystores::create_local_ca(ctx, &password, &subj, alg, d, stash))?
    } else {
        let seed = p.name("ROOTS") != "*NO";
        on(system, &ks, true, |ctx| keystores::create(ctx, &ks, stash, seed))?
    };
    Ok(reply(&notes, None))
}

fn dltcrts(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let notes = on(system, &ks, true, |ctx| keystores::delete(ctx, &ks))?;
    Ok(reply(&notes, None))
}

/// Read a stream file's bytes as the job.
fn read_stmf(system: &mut System, path: &str) -> Result<Vec<u8>, Note> {
    super::stmf::with_job(system, |c: &dyn IfsClient| {
        let fd = c.open(path, OpenArgs::new(O_RDONLY))?;
        let d = read_all(c, fd);
        let _ = c.close(fd);
        d
    })
    .map_err(|e: Errno| Note::new(msgs::FAILED, &[&format!("{path}: {}", e.text())]))
}

/// Write a stream file as the job, owner-only, tagged UTF-8.
fn write_stmf(system: &mut System, path: &str, bytes: &[u8], replace: bool) -> Result<(), Note> {
    let excl = if replace { O_TRUNC } else { O_EXCL };
    super::stmf::with_job(system, |c: &dyn IfsClient| {
        let fd = c.open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID | excl).mode(0o600).conv(1208))?;
        let w = write_all(c, fd, bytes);
        let _ = c.close(fd);
        w
    })
    .map_err(|e: Errno| Note::new(msgs::FAILED, &[&format!("{path}: {}", e.text())]))
}

fn crtcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let label = p.text("LABEL");
    let cn = p.text("CN");
    let kind = match p.name("USAGE").as_str() {
        "*CLIENT" => CertKind::Client,
        _ => CertKind::Server,
    };
    let mut san: Vec<String> = p
        .list("ALTNAME")
        .iter()
        .filter_map(|v| v.text().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("*CN") && !s.eq_ignore_ascii_case("*NONE"))
        .collect();
    let altname = p.name("ALTNAME");
    if san.is_empty() && altname != "*NONE" && kind == CertKind::Server {
        san.push(cn.clone());
    }
    let signer = match p.name("SIGNER").as_str() {
        "*LCLCA" => Signer::LocalCa(opt(p, "LCLCAPWD")),
        "*CSR" => Signer::Request,
        _ => Signer::SelfSigned,
    };
    let csr_path = opt(p, "CSR");
    if signer == Signer::Request && csr_path.is_none() {
        return Err(Note::new(msgs::FAILED, &["SIGNER(*CSR) needs CSR, the stream file for the request"]));
    }
    let spec = CertSpec {
        subject: subject(p, &cn),
        kind,
        san,
        not_before: crate::ipl::system::now_unix(),
        days: days(p, 365),
    };
    let alg = algorithm(p)?;
    let text = opt(p, "TEXT");
    let made = on(system, &ks, true, |ctx| {
        let m = entries::create(ctx, &ks, &label, &spec, alg, &signer)?;
        if let Some(t) = &text {
            entries::change(ctx, &ks, &label, &Change { text: Some(t.clone()), ..Change::default() })?;
        }
        Ok(m)
    })?;
    if let (Some(pem), Some(path)) = (&made.csr_pem, &csr_path) {
        write_stmf(system, path, pem.as_bytes(), true)?;
    }
    Ok(reply(&made.notes, None))
}

fn impcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let path = p.text("FROMSTMF");
    let bytes = read_stmf(system, &path)?;
    let label = p.text("LABEL");
    let format = match p.name("FORMAT").as_str() {
        "*PEM" => ImportFormat::Pem,
        "*PKCS12" => ImportFormat::Pkcs12,
        _ => ImportFormat::Auto,
    };
    let pw = opt(p, "IMPPWD");
    let trust = p.name("TRUST") != "*NO";
    let notes = on(system, &ks, true, |ctx| entries::import(ctx, &ks, &label, &bytes, format, pw.as_deref(), trust))?;
    Ok(reply(&notes, None))
}

fn expcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let label = p.text("LABEL");
    let path = p.text("TOSTMF");
    let format = match p.name("FORMAT").as_str() {
        "*PKCS12" => ExportFormat::Pkcs12(opt(p, "EXPPWD").unwrap_or_default()),
        "*CSR" => ExportFormat::Request,
        _ => ExportFormat::Pem {
            with_key: p.yes("INCKEY"),
        },
    };
    let bytes = on(system, &ks, false, |ctx| entries::export(ctx, &ks, &label, &format))?;
    write_stmf(system, &path, &bytes, p.yes("REPLACE"))?;
    Ok(reply(&[Note::new(msgs::EXPORTED, &[&label, &path])], None))
}

fn chgcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let label = p.text("LABEL");
    let c = Change {
        new_label: opt(p, "NEWLABEL"),
        trusted: match p.name("TRUST").as_str() {
            "*YES" => Some(true),
            "*NO" => Some(false),
            _ => None,
        },
        default: p.yes("DFTCRT"),
        text: match p.text("TEXT").as_str() {
            "*SAME" => None,
            "*BLANK" => Some(String::new()),
            t => Some(t.to_string()),
        },
    };
    let notes = on(system, &ks, true, |ctx| entries::change(ctx, &ks, &label, &c))?;
    Ok(reply(&notes, None))
}

fn dltcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let label = p.text("LABEL");
    let notes = on(system, &ks, true, |ctx| entries::remove(ctx, &ks, &label))?;
    Ok(reply(&notes, None))
}

/// `QSWRKCRT`'s text form.
fn wrkcrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let ks = keystore(p);
    let rows = on(system, &ks, false, |ctx| entries::list(ctx, &ks))?;
    let mut out = format!("Certificates in {}\n", ks.shown());
    let _ = writeln!(out, "{:<32} {:<8} {:<10} {:<9} Applications", "Label", "Type", "Expires", "Status");
    for r in &rows {
        let exp = r.info.as_ref().map_or_else(String::new, |i| iso_date(i.not_after)[..10].to_string());
        let _ = writeln!(
            out,
            "{:<32} {:<8} {:<10} {:<9} {}{}",
            r.label,
            r.kind.name(),
            exp,
            r.status(),
            if r.is_default { "(default) " } else { "" },
            r.apps.join(" ")
        );
    }
    Ok(CmdReply {
        text: out,
        ..CmdReply::default()
    })
}

fn asncrt(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let id = p.text("APPID");
    let label = opt(p, "LABEL");
    let ks = Keystore::system();
    let notes = on(system, &ks, true, |ctx| appids::assign(ctx, &id, label.as_deref()))?;
    Ok(reply(&notes, None))
}

fn addappid(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let id = p.text("APPID");
    let kind = if p.name("TYPE") == "*CLIENT" { AppKind::Client } else { AppKind::Server };
    let text = p.text("TEXT");
    let text = if text.eq_ignore_ascii_case("*BLANK") { String::new() } else { text };
    let notes = on(system, &Keystore::system(), true, |ctx| appids::add(ctx, &id, kind, &text))?;
    Ok(reply(&notes, None))
}

fn rmvappid(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let id = p.text("APPID");
    let notes = on(system, &Keystore::system(), true, |ctx| appids::remove(ctx, &id))?;
    Ok(reply(&notes, None))
}

fn chgappid(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let id = p.text("APPID");
    let trust: Vec<String> = p
        .list("CATRUST")
        .iter()
        .filter_map(|v| match v {
            Value::Char(s) => Some(s.trim().to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect();
    let ca_trust = match trust.first().map(|s| s.to_ascii_uppercase()) {
        None => None,
        Some(s) if s == "*SAME" => None,
        Some(s) if s == "*ALL" => Some(CaTrust::All),
        Some(_) => Some(CaTrust::Only(trust)),
    };
    let c = appids::Change {
        client_auth: client_auth_from(&p.name("CLTAUTH")),
        ca_trust,
        text: match p.text("TEXT").as_str() {
            "*SAME" | "" => None,
            "*BLANK" => Some(String::new()),
            t => Some(t.to_string()),
        },
    };
    let notes = on(system, &Keystore::system(), true, |ctx| appids::change(ctx, &id, &c))?;
    Ok(reply(&notes, None))
}

/// `QSWRKAPPID`'s text form.
fn wrkappid(system: &mut System, p: &Parms) -> Out {
    check_authority()?;
    let want = p.text("APPID");
    let apps = on(system, &Keystore::system(), false, appids::list)?;
    let mut out = String::from("Application IDs\n");
    let _ = writeln!(out, "{:<40} {:<8} {:<10} Certificate", "Application ID", "Type", "Client auth");
    for a in apps.iter().filter(|a| generic_match(&want, &a.id)) {
        let _ = writeln!(
            out,
            "{:<40} {:<8} {:<10} {}",
            a.id,
            kind_name(a.kind),
            client_auth_name(a.client_auth),
            a.label.as_deref().unwrap_or("*NONE")
        );
    }
    Ok(CmdReply {
        text: out,
        ..CmdReply::default()
    })
}

fn generic_match(pattern: &str, id: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() || p.eq_ignore_ascii_case("*ALL") {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => id.to_ascii_uppercase().starts_with(&prefix.to_ascii_uppercase()),
        None => id.eq_ignore_ascii_case(p),
    }
}

fn kind_name(k: AppKind) -> &'static str {
    match k {
        AppKind::Server => "*SERVER",
        AppKind::Client => "*CLIENT",
    }
}

fn chkcrt(system: &mut System) -> Out {
    check_authority()?;
    let lines = expiry_scan(system);
    let warned = lines.iter().filter(|l| l.starts_with(msgs::EXPIRES) || l.starts_with(msgs::EXPIRED)).count();
    let skipped = lines.len() - warned;
    let mut r = reply(&[Note::new(msgs::CHECKED, &[&warned.to_string(), &skipped.to_string()])], None);
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    r.text = text + &r.text;
    Ok(r)
}

/// `QSWRKCRT`'s panel data: the store and one row per certificate.
pub fn crt_rows(store: &str, rows: &[Row]) -> Sys2Data {
    let list = rows
        .iter()
        .map(|r| {
            vec![
                ("CRTLABEL".to_string(), r.label.clone()),
                ("CRTTYPE".to_string(), r.kind.name().to_string()),
                (
                    "CRTEXP".to_string(),
                    r.info.as_ref().map_or_else(String::new, |i| iso_date(i.not_after)[..10].to_string()),
                ),
                ("CRTSTS".to_string(), r.status().to_string()),
                ("CRTDFT".to_string(), if r.is_default { "Yes".into() } else { String::new() }),
            ]
        })
        .collect();
    (vec![("KEYSTORE".into(), store.into())], vec![("CERTS".into(), list)])
}

/// One certificate for its display.
pub fn crt_detail(store: &str, r: &Row) -> Sys2Data {
    let yes = |b: bool| if b { "Yes" } else { "No" }.to_string();
    let mut v = vec![
        ("KEYSTORE".to_string(), store.to_string()),
        ("DLABEL".to_string(), r.label.clone()),
        ("DTYPE".to_string(), r.kind.name().to_string()),
        ("DSTATUS".to_string(), r.status().to_string()),
        ("DDEFAULT".to_string(), yes(r.is_default)),
        ("DTEXT".to_string(), r.text.clone()),
        ("DAPPS".to_string(), if r.apps.is_empty() { "*NONE".into() } else { r.apps.join(" ") }),
    ];
    if let Some(i) = &r.info {
        let fp = i.sha256_colons();
        let (a, b) = fp.split_at(fp.len().min(48));
        v.extend([
            ("DSUBJECT".to_string(), i.subject.clone()),
            ("DISSUER".to_string(), i.issuer.clone()),
            ("DSERIAL".to_string(), i.serial.clone()),
            ("DFROM".to_string(), iso_date(i.not_before)),
            ("DTO".to_string(), iso_date(i.not_after)),
            ("DDAYS".to_string(), r.days_left.unwrap_or_default().to_string()),
            (
                "DKEY".to_string(),
                i.key_algorithm
                    .map_or_else(|| format!("{} bits", i.key_bits), |k| k.name().to_string()),
            ),
            ("DSAN".to_string(), i.san.join(" ")),
            ("DCA".to_string(), yes(i.is_ca)),
            ("DTRUST".to_string(), yes(r.trusted)),
            ("DSHA1".to_string(), a.trim_end_matches(':').to_string()),
            ("DSHA2".to_string(), b.trim_start_matches(':').to_string()),
        ]);
    }
    (v, Vec::new())
}

/// `QSWRKAPPID`'s panel data.
pub fn app_rows(apps: &[AppDef]) -> Sys2Data {
    let list = apps
        .iter()
        .map(|a| {
            vec![
                ("APPID".to_string(), a.id.clone()),
                ("APPTYPE".to_string(), kind_name(a.kind).to_string()),
                ("APPCRT".to_string(), a.label.clone().unwrap_or_else(|| "*NONE".into())),
                ("APPAUTH".to_string(), client_auth_name(a.client_auth).to_string()),
            ]
        })
        .collect();
    (Vec::new(), vec![("APPS".into(), list)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_data_fits_the_variables() {
        let d = data("TCP1C08", &["ADMIN".into(), "QIBM_HTTP_SERVER_ADMIN".into()]);
        assert_eq!(d.len(), 164);
        let t = text("TCP1C08", &d);
        assert!(t.contains("ADMIN") && t.contains("QIBM_HTTP_SERVER_ADMIN"), "{t}");
        assert!(text("TCP1C20", &data("TCP1C20", &["W".into(), "*SYSTEM".into(), "7".into()])).contains('7'));
    }

    #[test]
    fn generic_ids() {
        assert!(generic_match("*ALL", "QIBM_X"));
        assert!(generic_match("qibm_qzbs*", "QIBM_QZBS_SVR_FILE"));
        assert!(!generic_match("QIBM_QTV*", "QIBM_QZBS_SVR_FILE"));
        assert!(generic_match("QIBM_X", "QIBM_X"));
    }
}
