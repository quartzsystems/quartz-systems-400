// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The oracle's names masked out of a record, as the captures beside it
//! write them: `{system}` (the DNS name), `{host}` and `{domain}` (its
//! `dc=` parts), `{server-id}`.

use crate::dse::RootDse;

/// What to replace, longest first.
#[derive(Debug, Default)]
pub struct Mask {
    pairs: Vec<(String, &'static str)>,
}

impl Mask {
    /// The names `dse` and `host` reveal.
    pub fn from_dse(dse: &RootDse, host: &str) -> Mask {
        let mut m = Mask::default();
        m.add(host, "{system}");
        for v in dse.get("ibm-ldapservicename") {
            m.add(v, "{system}");
        }
        if let Some(s) = dse.system() {
            m.add(&s, "{system}");
        }
        for v in dse.get("ibm-serverId") {
            m.add(v, "{server-id}");
        }
        for ctx in dse.get("namingContexts") {
            let dcs: Vec<&str> = ctx
                .split(',')
                .filter_map(|p| p.trim().split_once('='))
                .filter(|(k, _)| k.eq_ignore_ascii_case("dc"))
                .map(|(_, v)| v)
                .collect();
            for (i, dc) in dcs.iter().enumerate() {
                m.add(dc, if i == 0 { "{host}" } else { "{domain}" });
            }
        }
        // The DNS name's first label is the host as well.
        let labels: Vec<String> = m
            .pairs
            .iter()
            .filter(|(_, r)| *r == "{system}")
            .filter_map(|(v, _)| v.split_once('.').map(|(h, _)| h.to_owned()))
            .filter(|h| h.starts_with(|c: char| c.is_ascii_alphabetic()))
            .collect();
        for l in labels {
            m.add(&l, "{host}");
        }
        m.pairs.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
        m
    }

    fn add(&mut self, v: &str, r: &'static str) {
        // Too short to mask without spoiling ordinary words.
        if v.len() >= 3 && !self.pairs.iter().any(|(p, _)| p.eq_ignore_ascii_case(v)) {
            self.pairs.push((v.to_owned(), r));
        }
    }

    /// `text` with every name replaced, case ignored.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (v, r) in &self.pairs {
            out = replace_ci(&out, v, r);
        }
        out
    }

    /// Whether any masked name is still in `text` (case ignored).
    pub fn leaks(&self, text: &str) -> bool {
        let low = text.to_ascii_lowercase();
        self.pairs.iter().any(|(v, _)| low.contains(&v.to_ascii_lowercase()))
    }
}

fn replace_ci(s: &str, pat: &str, with: &str) -> String {
    let low = s.to_ascii_lowercase();
    let lp = pat.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(j) = low[i..].find(&lp) {
        out.push_str(&s[i..i + j]);
        out.push_str(with);
        i += j + lp.len();
    }
    out.push_str(&s[i..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_every_form() {
        let mut d = RootDse::default();
        d.attrs.insert("ibm-ldapservicename".into(), vec!["box1.corp.example".into()]);
        d.attrs.insert("ibm-osregistrycontext".into(), vec!["OS400-SYS=BOX1.CORP.EXAMPLE".into()]);
        d.attrs.insert("ibm-serverid".into(), vec!["abc-123-def".into()]);
        d.attrs.insert(
            "namingcontexts".into(),
            vec!["CN=SCHEMA".into(), "dc=box1,dc=corp,dc=example".into()],
        );
        let m = Mask::from_dse(&d, "10.1.2.3");
        let t = m.apply("OS400-SYS=BOX1.CORP.EXAMPLE dc=box1,dc=corp,dc=example id abc-123-def at 10.1.2.3 cn=schema");
        assert_eq!(
            t,
            "OS400-SYS={system} dc={host},dc={domain},dc={domain} id {server-id} at {system} cn=schema"
        );
        assert!(!m.leaks(&t));
        assert!(m.leaks("Box1"));
    }
}
