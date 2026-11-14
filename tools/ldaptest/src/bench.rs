// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `bench`: binds a minute and a paged search's rate.

use std::time::Instant;

use ldap3::Scope;

use crate::client::{Conn, expect_rc};
use crate::conn::Tls;
use crate::scenarios::{Ctx, controls, read};
use crate::Error;

/// The bench's lines.
pub fn run(c: &Ctx) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let (dn, pw) = match &c.t.admin_pw {
        Some(p) => (c.t.admin_dn.clone(), p.clone()),
        None => (String::new(), String::new()),
    };
    let who = if dn.is_empty() { "anonymous" } else { dn.as_str() };
    let n = c.t.binds.max(1);
    let started = Instant::now();
    for _ in 0..n {
        let mut x = Conn::open(&c.t, Tls::None)?;
        let r = x.bind(&dn, &pw)?;
        expect_rc("bind", &r, 0)?;
        x.close();
    }
    out.push(format!("binds: {n} as {who}, a connection each, {:.0} a minute", per_minute(n, started)));
    let mut x = Conn::open(&c.t, Tls::None)?;
    let started = Instant::now();
    for _ in 0..n {
        let r = x.bind(&dn, &pw)?;
        expect_rc("bind", &r, 0)?;
    }
    out.push(format!("binds: {n} on one connection, {:.0} a minute", per_minute(n, started)));
    x.close();

    if c.t.suffix.is_some() && c.t.admin_pw.is_some() {
        let mut sb = c.sandbox("bench")?;
        let m = c.t.entries;
        let started = Instant::now();
        controls::fill(&mut sb, m)?;
        out.push(format!("adds: {m}, {:.0} a second", per_second(m, started)));
        let mut a = c.admin()?;
        let started = Instant::now();
        let p = controls::paged(&mut a, &sb.dn, Scope::OneLevel, "(objectClass=person)", c.t.page, &[])?;
        out.push(format!(
            "paged search: {} entries in {} pages of {}, {:.2} s, {:.0} entries a second ({})",
            p.dns.len(),
            p.pages,
            c.t.page,
            started.elapsed().as_secs_f64(),
            per_second(p.dns.len(), started),
            p.last
        ));
    } else if let Some(b) = read::bases(c).first() {
        let mut a = c.anon()?;
        let started = Instant::now();
        let p = controls::paged(&mut a, b, Scope::Subtree, "(objectClass=*)", c.t.page, &[])?;
        out.push(format!(
            "paged search of {b}: {} entries in {} pages, {:.3} s ({})",
            p.dns.len(),
            p.pages,
            started.elapsed().as_secs_f64(),
            p.last
        ));
    }
    Ok(out)
}

fn per_minute(n: usize, since: Instant) -> f64 {
    n as f64 * 60.0 / since.elapsed().as_secs_f64().max(1e-9)
}

fn per_second(n: usize, since: Instant) -> f64 {
    n as f64 / since.elapsed().as_secs_f64().max(1e-9)
}
