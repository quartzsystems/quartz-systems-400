// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv/inst/<name>/props`: the server properties (the honoured
//! attributes of the instance's `ibmslapd.conf`) with Apply and OK, and
//! the administrator; `/dirsrv/inst/<name>/admin` changes the
//! administrator or a password as `CHGDIRSVRA DN` does.

use dirsrv::admin::conf::PROPERTIES;
use dirsrv::config::ConfigError;
use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::partials::{Button, Prop, PropGroup, buttons, form, input, link, mono, props_table};

use super::{changed, fail, inst_tabs, inst_url, need, page, yes_no};
use crate::env::dirsrv::{AdminChange, AdminError, InstanceInfo, Properties, message};
use crate::routes::Rq;
use crate::routes::common::{pick, strings};

const SECURITY: &[&str] = &["none", "SSL", "TLS", "SSLTLS"];
const DEREF: &[&str] = &["never", "search", "find", "always"];
const CHECK: &[&str] = &["V3_lenient", "V3"];
const ENCRYPT: &[&str] = &["ssha512", "ssha256", "ssha", "sha", "crypt", "none"];

fn hint(h: &str) -> String {
    format!(" <span class=\"small grey\">{}</span>", esc(h))
}

fn yn(name: &str, b: bool) -> String {
    pick(
        name,
        &strings(&["*NO", "*YES"]),
        if b { "*YES" } else { "*NO" },
    )
}

/// `GLD0232` for a field `field` typed as `value`.
fn bad(field: &str, value: &str) -> AdminError {
    let attr = PROPERTIES
        .iter()
        .find(|(f, _, _)| *f == field)
        .map_or(field, |(_, _, a)| *a);
    ConfigError::BadValue {
        attr: attr.into(),
        value: value.into(),
    }
    .into()
}

/// The properties the form asks for.
fn posted(f: &Form) -> Result<Properties, AdminError> {
    let t = |k: &str| f.str(k).trim().to_string();
    let n = |k: &str| t(k).parse::<u32>().map_err(|_| bad(k, &t(k)));
    let port = |k: &str| t(k).parse::<u16>().map_err(|_| bad(k, &t(k)));
    let b = |k: &str| f.str(k) == "*YES";
    Ok(Properties {
        port: port("port")?,
        secure_port: port("secure_port")?,
        security: t("security"),
        size_limit: n("size_limit")?,
        time_limit: n("time_limit")?,
        idle_timeout: n("idle_timeout")?,
        write_timeout: n("write_timeout")?,
        allow_anon: b("allow_anon"),
        deref_aliases: t("deref_aliases"),
        schema_check: t("schema_check"),
        pw_encryption: t("pw_encryption"),
        read_only: b("read_only"),
        log_options: t("log_options"),
        audit: b("audit"),
        persistent_search: b("persistent_search"),
        max_persistent: n("max_persistent")?,
        paged_non_admin: b("paged_non_admin"),
        paged_limit: n("paged_limit")?,
        sort_non_admin: b("sort_non_admin"),
        sort_key_limit: n("sort_key_limit")?,
        changelog_max: n("changelog_max")?,
        pwd_policy: b("pwd_policy"),
        projected_read_only: b("projected_read_only"),
        referrals: t("referrals")
            .split_whitespace()
            .map(ToString::to_string)
            .collect(),
    })
}

#[allow(clippy::too_many_lines)]
fn groups(rq: &Rq<'_>, i: &InstanceInfo, p: &Properties, autostart: bool) -> Vec<PropGroup> {
    let num = |name: &str, v: u32, size: u32, h: &str| {
        let f = input(name, &v.to_string(), size);
        if h.is_empty() {
            f
        } else {
            format!("{f}{}", hint(h))
        }
    };
    let admin = rq.env().dirsrv.admin(rq.user(), &i.name).ok();
    let cert = if i.certificate.is_empty() {
        "None assigned: port 636 and StartTLS are off".to_string()
    } else {
        i.certificate.clone()
    };
    vec![
        PropGroup {
            title: Some("Instance".into()),
            rows: vec![
                Prop::new("Instance", mono(&i.name)),
                Prop::new(
                    "Configuration file",
                    mono(&dirsrv::config::conf_path(&i.location, &i.name)),
                ),
                Prop::new(
                    "Start automatically",
                    format!(
                        "{}{}",
                        yn("autostart", autostart),
                        hint("with STRTCPSVR SERVER(*AUTOSTART) and at IPL")
                    ),
                ),
            ],
        },
        PropGroup {
            title: Some("Connections".into()),
            rows: vec![
                Prop::new("Port", input("port", &p.port.to_string(), 5)),
                Prop::new(
                    "Secure port",
                    input("secure_port", &p.secure_port.to_string(), 5),
                ),
                Prop::new(
                    "Security",
                    format!(
                        "{}{}",
                        pick("security", &strings(SECURITY), &p.security),
                        hint("none: plain LDAP only")
                    ),
                ),
                Prop::new("Certificate application ID", mono(dirsrv::admin::APP_ID)),
                Prop::new("Certificate", esc(&cert)),
                Prop::new("Allow anonymous binds", yn("allow_anon", p.allow_anon)),
                Prop::new(
                    "Idle time-out (seconds)",
                    num("idle_timeout", p.idle_timeout, 7, "0: never"),
                ),
                Prop::new(
                    "Write time-out (seconds)",
                    num("write_timeout", p.write_timeout, 7, ""),
                ),
            ],
        },
        PropGroup {
            title: Some("Searches".into()),
            rows: vec![
                Prop::new(
                    "Size limit (entries)",
                    num("size_limit", p.size_limit, 9, "0: no limit"),
                ),
                Prop::new(
                    "Time limit (seconds)",
                    num("time_limit", p.time_limit, 9, "0: no limit"),
                ),
                Prop::new(
                    "Dereference aliases",
                    pick("deref_aliases", &strings(DEREF), &p.deref_aliases),
                ),
                Prop::new(
                    "Paged results for all users",
                    format!(
                        "{}{}",
                        yn("paged_non_admin", p.paged_non_admin),
                        hint("*NO: the administrator only")
                    ),
                ),
                Prop::new(
                    "Paged searches per connection",
                    num("paged_limit", p.paged_limit, 5, ""),
                ),
                Prop::new(
                    "Sorted results for all users",
                    yn("sort_non_admin", p.sort_non_admin),
                ),
                Prop::new(
                    "Sort keys at most",
                    num("sort_key_limit", p.sort_key_limit, 5, ""),
                ),
                Prop::new(
                    "Persistent search",
                    yn("persistent_search", p.persistent_search),
                ),
                Prop::new(
                    "Persistent searches at most",
                    num("max_persistent", p.max_persistent, 7, ""),
                ),
            ],
        },
        PropGroup {
            title: Some("Data".into()),
            rows: vec![
                Prop::new("Read only", yn("read_only", p.read_only)),
                Prop::new(
                    "Schema checking",
                    pick("schema_check", &strings(CHECK), &p.schema_check),
                ),
                Prop::new(
                    "Password encryption",
                    format!(
                        "{}{}",
                        pick("pw_encryption", &strings(ENCRYPT), &p.pw_encryption),
                        hint("how new userPassword values are kept")
                    ),
                ),
                Prop::new("Password policy", yn("pwd_policy", p.pwd_policy)),
                Prop::new(
                    "User profiles read only",
                    yn("projected_read_only", p.projected_read_only),
                ),
                Prop::new(
                    "Change log entries",
                    num("changelog_max", p.changelog_max, 9, "0: no change log"),
                ),
                Prop::new(
                    "Referrals",
                    format!(
                        "{}{}",
                        input("referrals", &p.referrals.join(" "), 60),
                        hint("LDAP URLs, separated by blanks")
                    ),
                ),
            ],
        },
        PropGroup {
            title: Some("Logging".into()),
            rows: vec![
                Prop::new("Log options", input("log_options", &p.log_options, 10)),
                Prop::new(
                    "Audit",
                    format!("{}{}", yn("audit", p.audit), hint("operations to QAUDJRN")),
                ),
            ],
        },
        PropGroup {
            title: Some("Administrator".into()),
            rows: admin.map_or_else(Vec::new, |a| {
                vec![
                    Prop::new("Administrator DN", mono(&a.dn)),
                    Prop::new(
                        "Password set",
                        format!(
                            "{} &nbsp; {}",
                            esc(yes_no(a.password_set)),
                            link(&inst_url(rq, i, "admin"), "Change...")
                        ),
                    ),
                    Prop::new("Update DN", mono(a.update_dn.as_deref().unwrap_or("*NONE"))),
                ]
            }),
        },
    ]
}

/// The properties page; `typed` what a refused post asked for.
fn show(rq: &Rq<'_>, i: &InstanceInfo, typed: Option<(&Properties, bool)>) -> Response {
    let p = match typed {
        Some((p, a)) => (p.clone(), a),
        None => match rq.env().dirsrv.properties(rq.user(), &i.name) {
            Ok(p) => (p, i.autostart),
            Err(e) => return fail(rq, &e, "Directory server properties", &[], "/dirsrv"),
        },
    };
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(rq.ctx.product, &groups(rq, i, &p.0, p.1), true),
            buttons(&[
                Button::new("apply", "Apply"),
                Button::new("ok", "OK"),
                Button::new("cancel", "Cancel"),
            ])
        ),
    );
    let title = format!("Properties of {}", i.name);
    page(
        rq,
        &["Instances", &i.name],
        &title,
        format!("{}{body}", inst_tabs(rq, i, 0)),
        "dsprops",
        false,
    )
}

/// `/dirsrv/inst/<name>/props` and its posts.
pub fn route(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    if !rq.post() {
        return show(rq, i, None);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/dirsrv");
    }
    let function = "Change directory server properties";
    if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
        return r;
    }
    let d = &rq.env().dirsrv;
    let base = match d.properties(rq.user(), &i.name) {
        Ok(p) => p,
        Err(e) => return fail(rq, &e, function, &[], "/dirsrv"),
    };
    let autostart = f.str("autostart") == "*YES";
    let to = match posted(&f) {
        Ok(p) => p,
        Err(e) => {
            rq.flash(message(&e));
            return show(rq, i, Some((&base, autostart)));
        }
    };
    let mut any = false;
    if autostart != i.autostart {
        if let Err(e) = d.set_autostart(rq.user(), &i.name, autostart) {
            return fail(rq, &e, function, &["Instances", &i.name], "/dirsrv");
        }
        any = true;
    }
    match d.set_properties(rq.user(), &i.name, &to) {
        Ok(attrs) => {
            any |= !attrs.is_empty();
            let back = if f.has("apply") {
                super::inst_route(i, "props")
            } else {
                "/dirsrv".to_string()
            };
            if any {
                changed(rq, i, &back)
            } else {
                rq.redirect(&back)
            }
        }
        Err(e) if e.kind == crate::env::dirsrv::ErrorKind::NotAuthorized => {
            fail(rq, &e, function, &["Instances", &i.name], "/dirsrv")
        }
        Err(e) => {
            rq.flash(message(&e));
            show(rq, i, Some((&to, autostart)))
        }
    }
}

/// `/dirsrv/inst/<name>/admin`: the administrator, its password or the
/// update DN's password.
pub fn admin(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let d = &rq.env().dirsrv;
    let info = match d.admin(rq.user(), &i.name) {
        Ok(a) => a,
        Err(e) => return fail(rq, &e, "Directory server administrator", &[], "/dirsrv"),
    };
    let back = super::inst_route(i, "props");
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        let function = "Change directory server administrator";
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        let change = match f.str("what") {
            "*UPDATEDN" => AdminChange::UpdatePassword,
            "*ADMINDN" => AdminChange::Password,
            _ => AdminChange::Dn {
                dn: f.str("dn").trim().to_string(),
            },
        };
        let (pw, again) = (f.str("pw"), f.str("pw2"));
        if pw != again {
            let m = rq.message(qsweb::msg::Kind::Error, "CPD9BAF", &[]);
            return rq.done(&super::inst_route(i, "admin"), m);
        }
        return match d.set_admin(rq.user(), &i.name, &change, pw) {
            Ok(()) => changed(rq, i, &back),
            Err(e) => fail(
                rq,
                &e,
                function,
                &["Instances", &i.name],
                &super::inst_route(i, "admin"),
            ),
        };
    }
    let pw = |n: &str| format!("<input type=\"password\" name=\"{n}\" size=\"30\">");
    let rows = vec![
        Prop::new("Administrator DN now", mono(&info.dn)),
        Prop::new(
            "Change",
            format!(
                "{}{}",
                pick(
                    "what",
                    &strings(&["*ADMINDN", "*NEWDN", "*UPDATEDN"]),
                    "*ADMINDN"
                ),
                hint(
                    "*ADMINDN: the password only; *NEWDN: the DN below and its password; *UPDATEDN: the update DN's password"
                )
            ),
        ),
        Prop::new("New administrator DN", input("dn", &info.dn, 50)),
        Prop::new("Password", pw("pw")).required(),
        Prop::new("Password again", pw("pw2")).required(),
    ];
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    let title = format!("Administrator of {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Administrator"],
        &title,
        body,
        "dsadmin",
        false,
    )
}
