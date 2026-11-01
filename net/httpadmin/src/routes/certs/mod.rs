// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Certificates: the stores and their entries, creating and importing
//! certificates, application IDs. Every page needs `*ALLOBJ` and
//! `*SECADM`, as the certificate commands do. Store passwords are used
//! for the one request that needs them and never logged, kept in the
//! session, sent in a message or put on a page.

pub mod apps;
pub mod cert;
pub mod create;
pub mod import;
pub mod stores;

use qsweb::html::query;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};

use super::Rq;
use crate::PRODUCT;
use crate::env::{CertFailure, CertRow, Need};
use crate::msgs;

/// `/certs/...`.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    let segs: Vec<String> = segs.iter().map(|s| qsweb::http::form::decode(s, false)).collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    if let Err(r) = rq.require_each(
        &[Need::AllObj, Need::SecAdm],
        "Work with certificates",
        "*SYSTEM",
        &["Certificates"],
    ) {
        return r;
    }
    match segs.as_slice() {
        ["stores"] => stores::list(rq),
        ["store", name] => stores::store(rq, name, stores::Tab::Server),
        ["store", name, "cas"] => stores::store(rq, name, stores::Tab::Ca),
        ["store", name, "requests"] => stores::store(rq, name, stores::Tab::Request),
        ["store", name, "password"] => stores::password(rq, name),
        ["store", name, "default"] => stores::set_default(rq, name),
        ["cert"] => cert::view(rq),
        ["download"] => cert::download(rq),
        ["confirm"] => cert::confirm(rq),
        ["create", "step", n] => create::step(rq, n),
        ["create", "result"] => create::result(rq),
        ["import"] => import::page(rq),
        ["apps"] => apps::page(rq),
        _ => rq.not_found(&rq.req.path),
    }
}

/// The page of store `name` (`tab` "" for server certificates).
pub fn store_url(name: &str, tab: &str) -> String {
    if tab.is_empty() {
        PRODUCT.url(&format!("/certs/store/{name}"))
    } else {
        PRODUCT.url(&format!("/certs/store/{name}/{tab}"))
    }
}

/// `route` with a query of `pairs`.
pub fn url_q(route: &str, pairs: &[(&str, &str)]) -> String {
    format!("{}{}", PRODUCT.url(route), query(pairs))
}

/// A failure as the message row it shows as.
pub fn failed(f: &CertFailure) -> Message {
    match f {
        CertFailure::BadPassword(s) => msgs::message(Kind::Error, "HTP8321", &[s]),
        other => msgs::message(Kind::Error, "HTP8320", &[&other.to_string()]),
    }
}

/// A form value that is not valid.
pub fn invalid(what: &str) -> Message {
    msgs::message(Kind::Error, "HTP8322", &[what])
}

/// An RFC 4514 name as pages show it: `CN=a, O=b, C=US`.
pub fn dn(s: &str) -> String {
    rdns(s).join(", ")
}

/// The attributes of an RFC 4514 name, split at unescaped commas.
fn rdns(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for c in s.chars() {
        if esc {
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            cur.push(c);
            esc = true;
        } else if c == ',' {
            parts.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    parts.push(cur.trim().to_string());
    parts
}

/// The common name of an RFC 4514 name, else the whole name.
pub fn cn(s: &str) -> String {
    rdns(s)
        .iter()
        .find_map(|p| p.strip_prefix("CN="))
        .map_or_else(|| dn(s), |c| c.replace('\\', ""))
}

/// Whole days left at `now`; negative once expired.
pub fn days_left(row: &CertRow, now: u64) -> i64 {
    (i64::try_from(row.not_after).unwrap_or(i64::MAX) - i64::try_from(now).unwrap_or(0)).div_euclid(86_400)
}

/// Days within which an expiry shows in red.
pub const WARN_DAYS: i64 = 60;

/// The status of a certificate: Expired, Expires in N days (red), or
/// Active (green); `None` for a request.
pub fn expiry_status(row: &CertRow, now: u64) -> Option<String> {
    if row.not_after == 0 {
        return None;
    }
    let d = days_left(row, now);
    Some(if row.not_after < now {
        qsweb::partials::status("Expired", Some(false))
    } else if d <= WARN_DAYS {
        let n = if d == 1 { "1 day".to_string() } else { format!("{d} days") };
        qsweb::partials::status(&format!("Expires in {n}"), Some(false))
    } else {
        qsweb::partials::status("Active", Some(true))
    })
}

/// The issuer as the store page shows it: `(self-signed)`, the label of
/// the store's CA entry for it, or its common name.
pub fn issuer_name(row: &CertRow, all: &[CertRow]) -> String {
    if row.self_signed {
        return "(self-signed)".into();
    }
    all.iter()
        .find(|r| r.subject == row.issuer && r.label != row.label && r.kind == crate::env::EntryType::Ca)
        .map_or_else(|| cn(&row.issuer), |r| r.label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(dn("CN=a\\, b,O=Quartz Systems,C=US"), "CN=a\\, b, O=Quartz Systems, C=US");
        assert_eq!(cn("CN=a\\, b,O=x"), "a, b");
        assert_eq!(cn("O=x"), "O=x");
    }
}
