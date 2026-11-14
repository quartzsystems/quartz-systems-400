// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A connection with a timeout on every operation, results kept whole
//! (entries, references, the result and its controls), and a read-only
//! guard: in oracle mode it refuses every write and every bind with a
//! password before anything is sent.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Duration;

use ldap3::controls::RawControl;
use ldap3::exop::Exop;
use ldap3::{LdapConn, LdapResult, Mod, ResultEntry, Scope, SearchEntry, SearchOptions};

use crate::conn::{self, Tls};
use crate::{Error, Target, code};

/// A search's answer.
#[derive(Debug)]
pub struct Found {
    /// The entries, in the order they came.
    pub entries: Vec<SearchEntry>,
    /// Each entry's controls.
    pub entry_ctrls: Vec<Vec<ldap3::controls::Control>>,
    /// Continuation references' URLs.
    pub refs: Vec<Vec<String>>,
    /// The result.
    pub res: LdapResult,
}

impl Found {
    /// The entries' DNs.
    pub fn dns(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.dn.clone()).collect()
    }
}

/// A search's parameters beyond base, scope and filter.
#[derive(Clone, Debug, Default)]
pub struct Opts {
    /// Size limit (0: none).
    pub size: i32,
    /// Time limit in seconds (0: none).
    pub time: i32,
    /// Types only.
    pub types_only: bool,
    /// Controls.
    pub ctrls: Vec<RawControl>,
}

/// One connection.
pub struct Conn {
    c: LdapConn,
    timeout: Duration,
    read_only: bool,
}

impl Conn {
    /// A new anonymous connection to `t`.
    pub fn open(t: &Target, tls: Tls) -> Result<Conn, Error> {
        Self::open_with(t, tls, false)
    }

    /// A new connection, presenting the client certificate when `with_cert`.
    pub fn open_with(t: &Target, tls: Tls, with_cert: bool) -> Result<Conn, Error> {
        Ok(Conn {
            c: conn::open_with(t, tls, with_cert)?,
            timeout: t.timeout,
            read_only: t.is_oracle(),
        })
    }

    /// The underlying connection, with the timeout set for its next operation.
    pub fn raw(&mut self) -> &mut LdapConn {
        self.c.with_timeout(self.timeout);
        &mut self.c
    }

    fn writable(&self, what: &str) -> Result<(), Error> {
        if self.read_only {
            Err(Error::Check(format!("{what} refused: read-only run")))
        } else {
            Ok(())
        }
    }

    /// A simple bind. Refused in oracle mode unless anonymous.
    pub fn bind(&mut self, dn: &str, pw: &str) -> Result<LdapResult, Error> {
        if self.read_only && !(dn.is_empty() && pw.is_empty()) {
            return Err(Error::Check(
                "bind refused: read-only run binds anonymously only".into(),
            ));
        }
        Ok(self.raw().simple_bind(dn, pw)?)
    }

    /// A SASL EXTERNAL bind. Refused in oracle mode.
    pub fn external(&mut self) -> Result<LdapResult, Error> {
        self.writable("SASL bind")?;
        Ok(self.raw().sasl_external_bind()?)
    }

    /// A search.
    pub fn search(
        &mut self,
        base: &str,
        scope: Scope,
        filter: &str,
        attrs: &[&str],
        o: &Opts,
    ) -> Result<Found, Error> {
        let opts = SearchOptions::new()
            .sizelimit(o.size)
            .timelimit(o.time)
            .typesonly(o.types_only);
        let c = self.raw();
        c.with_search_options(opts);
        if !o.ctrls.is_empty() {
            c.with_controls(o.ctrls.clone());
        }
        // No adapters: `search()`'s `EntriesOnly` drops the continuation
        // references the referral scenarios look for.
        let none: Vec<Box<dyn ldap3::adapters::Adapter<'_, &str, Vec<&str>>>> = Vec::new();
        let mut st = c.streaming_search_with(none, base, scope, filter, attrs.to_vec())?;
        let mut raw = Vec::new();
        while let Some(re) = st.next()? {
            raw.push(re);
        }
        let res = st.result();
        Ok(split(raw, res))
    }

    /// A search with no options.
    pub fn find(
        &mut self,
        base: &str,
        scope: Scope,
        filter: &str,
        attrs: &[&str],
    ) -> Result<Found, Error> {
        self.search(base, scope, filter, attrs, &Opts::default())
    }

    /// A compare.
    pub fn compare(&mut self, dn: &str, attr: &str, val: &str) -> Result<LdapResult, Error> {
        Ok(self.raw().compare(dn, attr, val)?.0)
    }

    /// An extended operation, with controls.
    pub fn extended(
        &mut self,
        exop: Exop,
        ctrls: Vec<RawControl>,
    ) -> Result<(Exop, LdapResult), Error> {
        let c = self.raw();
        if !ctrls.is_empty() {
            c.with_controls(ctrls);
        }
        let r = c.extended(exop)?;
        Ok((r.0, r.1))
    }

    /// An add. Refused in oracle mode.
    pub fn add(
        &mut self,
        dn: &str,
        attrs: &[(&str, &[&str])],
        ctrls: Vec<RawControl>,
    ) -> Result<LdapResult, Error> {
        self.writable("add")?;
        let v: Vec<(String, HashSet<String>)> = attrs
            .iter()
            .map(|(n, vals)| {
                (
                    (*n).to_owned(),
                    vals.iter().map(|s| (*s).to_owned()).collect(),
                )
            })
            .collect();
        let c = self.raw();
        if !ctrls.is_empty() {
            c.with_controls(ctrls);
        }
        Ok(c.add(dn, v)?)
    }

    /// An add of byte values. Refused in oracle mode.
    pub fn add_bytes(
        &mut self,
        dn: &str,
        attrs: Vec<(Vec<u8>, HashSet<Vec<u8>>)>,
    ) -> Result<LdapResult, Error> {
        self.writable("add")?;
        Ok(self.raw().add(dn, attrs)?)
    }

    /// A modify. Refused in oracle mode.
    pub fn modify(
        &mut self,
        dn: &str,
        mods: Vec<Mod<String>>,
        ctrls: Vec<RawControl>,
    ) -> Result<LdapResult, Error> {
        self.writable("modify")?;
        let c = self.raw();
        if !ctrls.is_empty() {
            c.with_controls(ctrls);
        }
        Ok(c.modify(dn, mods)?)
    }

    /// A modify DN. Refused in oracle mode.
    pub fn moddn(
        &mut self,
        dn: &str,
        rdn: &str,
        delete_old: bool,
        sup: Option<&str>,
    ) -> Result<LdapResult, Error> {
        self.writable("modify DN")?;
        Ok(self.raw().modifydn(dn, rdn, delete_old, sup)?)
    }

    /// A delete. Refused in oracle mode.
    pub fn delete(&mut self, dn: &str, ctrls: Vec<RawControl>) -> Result<LdapResult, Error> {
        self.writable("delete")?;
        let c = self.raw();
        if !ctrls.is_empty() {
            c.with_controls(ctrls);
        }
        Ok(c.delete(dn)?)
    }

    /// Unbinds; errors ignored.
    pub fn close(mut self) {
        let _ = self.c.unbind();
    }
}

/// Splits raw entries into entries and references.
pub fn split(raw: Vec<ResultEntry>, res: LdapResult) -> Found {
    let mut f = Found {
        entries: Vec::new(),
        entry_ctrls: Vec::new(),
        refs: Vec::new(),
        res,
    };
    for re in raw {
        if re.is_ref() {
            f.refs.push(ldap3::parse_refs(re.0));
        } else if !re.is_intermediate() {
            f.entry_ctrls.push(re.1.clone());
            f.entries.push(SearchEntry::construct(re));
        }
    }
    f
}

/// `rc N name`, with `matched "DN"` when there is one.
pub fn rc(res: &LdapResult) -> String {
    let mut s = format!("rc {} {}", res.rc, code::name(res.rc));
    if !res.matched.is_empty() {
        let _ = write!(s, " matched \"{}\"", res.matched);
    }
    if !res.refs.is_empty() {
        let _ = write!(s, " refs {:?}", res.refs);
    }
    s
}

/// `rc` plus the diagnostic text on its own detail line (server wording,
/// which a diff of two servers masks).
pub fn rc_lines(what: &str, res: &LdapResult, out: &mut Vec<String>) {
    out.push(format!("{what}: {}", rc(res)));
    if !res.text.is_empty() {
        out.push(format!("{what} diag: {}", res.text.trim()));
    }
}

/// An attribute map's names, lower-cased and sorted.
pub fn names(
    attrs: &HashMap<String, Vec<String>>,
    bin: &HashMap<String, Vec<Vec<u8>>>,
) -> Vec<String> {
    let mut v: Vec<String> = attrs
        .keys()
        .chain(bin.keys())
        .map(|k| k.to_ascii_lowercase())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The values of `name` in `e`, case of the name ignored.
pub fn values<'a>(e: &'a SearchEntry, name: &str) -> Vec<&'a str> {
    e.attrs
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(name))
        .flat_map(|(_, v)| v.iter().map(String::as_str))
        .collect()
}

/// Fails unless `res` has code `want`.
pub fn expect_rc(what: &str, res: &LdapResult, want: u32) -> Result<(), Error> {
    crate::error::check(res.rc == want, || {
        format!(
            "{what}: {}, wanted {want} {}{}",
            rc(res),
            code::name(want),
            diag(res)
        )
    })
}

fn diag(res: &LdapResult) -> String {
    if res.text.is_empty() {
        String::new()
    } else {
        format!(" ({})", res.text.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An oracle run's connection refuses writes and password binds before
    /// anything is sent (the listener never answers).
    #[test]
    fn oracle_connections_are_read_only() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port().to_string();
        let args: Vec<String> = ["oracle", "--port", &port, "--timeout", "2"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let env = |k: &str| (k == "QS_ORACLE_HOST").then(|| "127.0.0.1".to_owned());
        let t = Target::from_args(&args, &env).unwrap();
        let mut c = Conn::open(&t, Tls::None).unwrap();
        let refused = |r: Result<LdapResult, Error>| matches!(r, Err(Error::Check(m)) if m.contains("refused"));
        assert!(refused(c.bind("cn=Administrator", "x")));
        assert!(refused(c.bind("cn=x", "")));
        assert!(refused(c.add("cn=x", &[("cn", &["x"])], vec![])));
        assert!(refused(c.modify("cn=x", vec![], vec![])));
        assert!(refused(c.moddn("cn=x", "cn=y", true, None)));
        assert!(refused(c.delete("cn=x", vec![])));
        assert!(refused(c.external()));
        assert!(refused(c.add_bytes("cn=x", vec![])));
    }
}
