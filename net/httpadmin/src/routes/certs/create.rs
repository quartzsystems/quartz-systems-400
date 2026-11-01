// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Create certificate (`cert-create-1.html` to `-3.html`,
//! `cert-create-result.html`): key, subject, signing, result. The
//! wizard's choices live in the session under `certnew.`; no password
//! is ever needed or kept (stores open through their stash).

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, Prop, PropGroup, checkbox, input, message_row, props_table, radio, select, wizard};

use super::cert::{download_buttons, download_target, facts};
use super::{Rq, failed, invalid, rdns, store_url};
use crate::PRODUCT;
use crate::env::{CertDetail, Dn, EntryType, KeySpec, NewCert, Signing};
use crate::msgs;

const STEPS: [&str; 4] = ["Key type and size", "Subject", "Signing", "Result"];
const P: &str = "certnew.";

/// The fields each step posts.
const FIELDS: [&[&str]; 3] = [
    &["label", "kt", "ks", "curve", "sig", "days"],
    &["cn", "san", "ou", "o", "l", "st", "c"],
    &["s", "ca", "ku1", "ku2", "dft"],
];

/// The checkboxes of step 3, which post nothing when clear.
const CHECKS: [&str; 3] = ["ku1", "ku2", "dft"];

fn get(rq: &Rq<'_>, k: &str) -> String {
    rq.get(&format!("{P}{k}"))
}

fn store(rq: &Rq<'_>) -> String {
    let s = get(rq, "store");
    if s.is_empty() { "*SYSTEM".into() } else { s }
}

/// A select whose first option is the browser's choice anyway.
fn sel(name: &str, opts: &[&str], chosen: &str) -> String {
    select(name, opts, if opts.first() == Some(&chosen) { "" } else { chosen })
}

/// Start over for `store`, prefilled from certificate `renew` if given.
fn start(rq: &Rq<'_>, store: &str, renew: &str) {
    rq.clear(P);
    rq.clear("certres.");
    let k = |f: &str| format!("{P}{f}");
    rq.set(&[(k("store").as_str(), store)]);
    if renew.is_empty() {
        return;
    }
    let Ok(d) = rq.env().certs.detail(store, renew) else { return };
    let (kt, ks, curve) = match d.row.key.as_str() {
        "ECDSA P-256" => ("ec", "2048", "P-256"),
        "ECDSA P-384" => ("ec", "2048", "P-384"),
        "RSA 3072" => ("rsa", "3072", "P-256"),
        "RSA 4096" => ("rsa", "4096", "P-256"),
        _ => ("rsa", "2048", "P-256"),
    };
    let sig = if curve == "P-384" { "SHA-384" } else { "SHA-256" };
    let label = format!("{renew}-new");
    let mut pairs: Vec<(String, String)> = vec![
        (k("label"), label),
        (k("kt"), kt.into()),
        (k("ks"), ks.into()),
        (k("curve"), curve.into()),
        (k("sig"), sig.into()),
        (k("san"), d.san.join(", ")),
    ];
    for part in rdns(&d.row.subject) {
        if let Some((a, v)) = part.split_once('=') {
            let f = match a {
                "CN" => "cn",
                "OU" => "ou",
                "O" => "o",
                "L" => "l",
                "ST" => "st",
                "C" => "c",
                _ => continue,
            };
            pairs.push((k(f), v.replace('\\', "")));
        }
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    rq.set(&refs);
}

/// Keep what step `n` posted.
fn keep(rq: &Rq<'_>, n: usize) {
    let f = rq.form();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for name in FIELDS[n - 1] {
        let v = if CHECKS.contains(name) {
            if f.has(name) { "on" } else { "off" }.to_string()
        } else {
            f.str(name).trim().to_string()
        };
        pairs.push((format!("{P}{name}"), v));
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    rq.set(&refs);
}

fn key_spec(rq: &Rq<'_>) -> KeySpec {
    if get(rq, "kt") == "ec" {
        if get(rq, "curve") == "P-384" { KeySpec::P384 } else { KeySpec::P256 }
    } else {
        match get(rq, "ks").as_str() {
            "3072" => KeySpec::Rsa3072,
            "4096" => KeySpec::Rsa4096,
            _ => KeySpec::Rsa2048,
        }
    }
}

/// Why step `n`'s values cannot be used.
fn check(rq: &Rq<'_>, n: usize) -> Option<String> {
    match n {
        1 => {
            let label = get(rq, "label");
            if label.is_empty() || label.chars().count() > 64 {
                return Some("a certificate label is 1 to 64 characters".into());
            }
            let st = store(rq);
            if rq.env().certs.entries(&st).is_ok_and(|rows| rows.iter().any(|r| r.label == label)) {
                return Some(format!("store {st} already has a certificate labeled {label}"));
            }
            let days: u32 = get(rq, "days").parse().unwrap_or(0);
            if !(1..=9999).contains(&days) {
                return Some("the validity is 1 to 9999 days".into());
            }
            let p384 = key_spec(rq) == KeySpec::P384;
            if (get(rq, "sig") == "SHA-384") != p384 {
                return Some(if p384 {
                    "an ECDSA P-384 key signs with SHA-384".into()
                } else {
                    "SHA-384 signatures need an ECDSA P-384 key; RSA and P-256 keys sign with SHA-256".into()
                });
            }
            None
        }
        2 => {
            if get(rq, "cn").is_empty() {
                return Some("a common name is required".into());
            }
            let c = get(rq, "c");
            if !c.is_empty() && (c.len() != 2 || !c.chars().all(|x| x.is_ascii_alphabetic())) {
                return Some("the country is a two-letter code".into());
            }
            None
        }
        _ => {
            let s = get(rq, "s");
            if s == "local" && !rq.env().certs.signers().contains(&get(rq, "ca")) {
                return Some("choose a certificate authority of this system".into());
            }
            if !matches!(s.as_str(), "self" | "local" | "csr") {
                return Some("choose how the certificate is signed".into());
            }
            if get(rq, "ku1") == "off" && get(rq, "ku2") != "on" {
                return Some("choose at least one key usage".into());
            }
            None
        }
    }
}

fn page(rq: &Rq<'_>, n: usize, body: &str, bs: &[Button]) -> Response {
    let st = store(rq);
    rq.page(Page {
        title: "Create certificate".into(),
        crumb: vec![
            "Certificates".into(),
            "Certificate stores".into(),
            st,
            "Create certificate".into(),
        ],
        h1: "Create certificate".into(),
        desc: Some(format!("Step {n} of 4.")),
        body: wizard(&rq.ctx, &STEPS, n, body, bs),
        help: "cert-create".into(),
        ..Page::default()
    })
}

fn nav_buttons(n: usize) -> Vec<Button> {
    let back = Button::new("back", "< Back");
    let cancel = Button::new("cancel", "Cancel");
    match n {
        1 => vec![back.disabled(), Button::new("next", "Next >"), cancel],
        2 => vec![back, Button::new("next", "Next >"), cancel],
        _ => vec![back, cancel, Button::new("finish", "Create")],
    }
}

fn step1(rq: &Rq<'_>) -> String {
    let or = |k: &str, d: &str| {
        let v = get(rq, k);
        if v.is_empty() { d.to_string() } else { v }
    };
    let ec = or("kt", "rsa") == "ec";
    let rows = vec![
        Prop::new("Certificate label", input("label", &get(rq, "label"), 30)).required(),
        Prop::new(
            "Key type",
            format!("{} RSA<br>{} ECDSA", radio("kt", "rsa", !ec), radio("kt", "ec", ec)),
        ),
        Prop::new(
            "Key size",
            format!(
                "{} bits (RSA) &nbsp; {} (ECDSA)",
                sel("ks", &["2048", "3072", "4096"], &or("ks", "2048")),
                sel("curve", &["P-256", "P-384"], &or("curve", "P-256"))
            ),
        ),
        Prop::new("Signature algorithm", sel("sig", &["SHA-256", "SHA-384"], &or("sig", "SHA-256"))),
        Prop::new("Validity", format!("{} days", input("days", &or("days", "365"), 5))),
    ];
    format!(
        "<h2>Key type and size</h2>{}",
        props_table(&PRODUCT, &[PropGroup { title: None, rows }], false)
    )
}

fn step2(rq: &Rq<'_>) -> String {
    let rows = vec![
        Prop::new("Common name (CN)", input("cn", &get(rq, "cn"), 40)).required(),
        Prop::new(
            "Subject alternative names",
            format!("{} <span class=\"small grey\">comma separated</span>", input("san", &get(rq, "san"), 50)),
        ),
        Prop::new("Organizational unit (OU)", input("ou", &get(rq, "ou"), 30)),
        Prop::new("Organization (O)", input("o", &get(rq, "o"), 30)),
        Prop::new("Locality (L)", input("l", &get(rq, "l"), 30)),
        Prop::new("State or province (ST)", input("st", &get(rq, "st"), 30)),
        Prop::new("Country (C)", input("c", &get(rq, "c"), 2)),
    ];
    format!(
        "<h2>Subject</h2><p class=\"small grey\">The distinguished name that appears in the certificate.</p>{}",
        props_table(&PRODUCT, &[PropGroup { title: None, rows }], false)
    )
}

fn step3(rq: &Rq<'_>) -> String {
    let signers = rq.env().certs.signers();
    let s = get(rq, "s");
    let s = if s.is_empty() { if signers.is_empty() { "self".into() } else { "local".into() } } else { s };
    let mut sign = format!("{} Self-signed certificate<br>", radio("s", "self", s == "self"));
    if !signers.is_empty() {
        let opts: Vec<&str> = signers.iter().map(String::as_str).collect();
        let _ = write!(
            sign,
            "{} Local certificate authority: {}<br>",
            radio("s", "local", s == "local"),
            sel("ca", &opts, &get(rq, "ca"))
        );
    }
    let _ = write!(
        sign,
        "{} Generate a certificate signing request (CSR) for an external CA",
        radio("s", "csr", s == "csr")
    );
    let on = |k: &str, d: bool| match get(rq, k).as_str() {
        "on" => true,
        "off" => false,
        _ => d,
    };
    let rows = vec![
        Prop::new("Sign with", sign),
        Prop::new(
            "Key usage",
            format!(
                "{} Server authentication<br>{} Client authentication",
                checkbox("ku1", on("ku1", true)),
                checkbox("ku2", on("ku2", false))
            ),
        ),
        Prop::new(
            "After creation",
            format!("{} Make this the default certificate for the store", checkbox("dft", on("dft", false))),
        ),
    ];
    format!("<h2>Signing</h2>{}", props_table(&PRODUCT, &[PropGroup { title: None, rows }], false))
}

/// The certificate the wizard describes.
fn new_cert(rq: &Rq<'_>) -> NewCert {
    let s = get(rq, "s");
    NewCert {
        store: store(rq),
        label: get(rq, "label"),
        key: key_spec(rq),
        days: get(rq, "days").parse().unwrap_or(365),
        subject: Dn {
            cn: get(rq, "cn"),
            ou: get(rq, "ou"),
            o: get(rq, "o"),
            l: get(rq, "l"),
            st: get(rq, "st"),
            c: get(rq, "c").to_ascii_uppercase(),
        },
        san: get(rq, "san").split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
        server: get(rq, "ku1") != "off",
        signing: match s.as_str() {
            "self" => Signing::SelfSigned,
            "csr" => Signing::Request,
            _ => Signing::Ca(get(rq, "ca")),
        },
        make_default: get(rq, "dft") == "on",
    }
}

fn finish(rq: &Rq<'_>) -> Response {
    for n in 1..=3 {
        if let Some(why) = check(rq, n) {
            return rq.done(&format!("/certs/create/step/{n}"), invalid(&why));
        }
    }
    let new = new_cert(rq);
    match rq.env().certs.create(rq.user(), &new) {
        Ok(d) => {
            let signer = match &new.signing {
                Signing::SelfSigned => "its own key".to_string(),
                Signing::Ca(n) => n.clone(),
                Signing::Request => String::new(),
            };
            rq.audit_change(&new.store, &format!("certificate {} created", new.label));
            rq.clear(P);
            rq.set(&[
                ("certres.store", &new.store),
                ("certres.label", &d.row.label),
                ("certres.signer", &signer),
            ]);
            rq.redirect("/certs/create/result")
        }
        Err(e) => rq.done("/certs/create/step/3", failed(&e)),
    }
}

/// `/certs/create/step/<n>`.
pub fn step(rq: &Rq<'_>, n: &str) -> Response {
    let Some(n) = n.parse::<usize>().ok().filter(|n| (1..=3).contains(n)) else {
        return rq.not_found("such step of Create certificate");
    };
    let q = rq.query();
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            let back = store_url(&store(rq), "");
            rq.clear(P);
            return Response::redirect(&back);
        }
        if !(f.has("next") || f.has("back") || f.has("finish")) {
            // Create certificate... on a store page: start for that store.
            let st = q.get("store").map_or_else(|| "*SYSTEM".to_string(), ToString::to_string);
            start(rq, &st, "");
            return rq.redirect("/certs/create/step/1");
        }
        keep(rq, n);
        if f.has("back") {
            return rq.redirect(&format!("/certs/create/step/{}", n.saturating_sub(1).max(1)));
        }
        if f.has("finish") && n == 3 {
            return finish(rq);
        }
        if let Some(why) = check(rq, n) {
            return rq.done(&format!("/certs/create/step/{n}"), invalid(&why));
        }
        rq.set(&[(format!("{P}done{n}").as_str(), "1")]);
        return rq.redirect(&format!("/certs/create/step/{}", (n + 1).min(3)));
    }
    if let Some(st) = q.get("store") {
        start(rq, st, q.str("renew"));
        return rq.redirect("/certs/create/step/1");
    }
    // A step whose earlier steps are not done goes back to the first.
    if (1..n).any(|m| get(rq, &format!("done{m}")) != "1") {
        return rq.redirect("/certs/create/step/1");
    }
    let body = match n {
        1 => step1(rq),
        2 => step2(rq),
        _ => step3(rq),
    };
    page(rq, n, &body, &nav_buttons(n))
}

fn result_body(rq: &Rq<'_>, d: &CertDetail, store: &str, signer: &str) -> String {
    let m = if d.row.kind == EntryType::Request {
        msgs::message(Kind::Done, "HTP8303", &[&d.row.label, store])
    } else {
        msgs::message(Kind::Done, "HTP8301", &[&d.row.label, store, signer])
    };
    format!(
        "<h2>Result</h2>{}{}{}",
        message_row(&PRODUCT, &m),
        facts(rq, d),
        download_buttons(d)
    )
}

/// `/certs/create/result`: what was made, its PEM and downloads.
pub fn result(rq: &Rq<'_>) -> Response {
    let st = rq.get("certres.store");
    let label = rq.get("certres.label");
    if st.is_empty() {
        return rq.redirect("/certs/create/step/1");
    }
    if rq.post() {
        if let Some(r) = download_target(rq, &st, &label) {
            return r;
        }
        rq.clear("certres.");
        return Response::redirect(&store_url(&st, ""));
    }
    let d = match rq.env().certs.detail(&st, &label) {
        Ok(d) => d,
        Err(e) => return rq.done("/certs/stores", failed(&e)),
    };
    let body = result_body(rq, &d, &st, &rq.get("certres.signer"));
    let bs = [
        Button::new("back", "< Back").disabled(),
        Button::new("cancel", "Cancel"),
        Button::new("finish", "Done"),
    ];
    rq.page(Page {
        title: "Create certificate".into(),
        crumb: vec![
            "Certificates".into(),
            "Certificate stores".into(),
            st.clone(),
            "Create certificate".into(),
        ],
        h1: "Create certificate".into(),
        desc: Some("Step 4 of 4.".into()),
        body: wizard(&rq.ctx, &STEPS, 4, &body, &bs),
        help: "cert-create".into(),
        ..Page::default()
    })
}
