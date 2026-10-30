// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The relational database directory: one entry per RDB a job can
//! `CONNECT TO`, the `*LOCAL` one naming this system. Defaults are
//! `ADDRDBDIRE`'s; a directory never written holds the `*LOCAL` entry
//! for the system name (made at install).

use super::Refused;
use super::text::{get, kv, parse};
use crate::sec::Mechanism;

/// One directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RdbEntry {
    /// `RDB`: the relational database.
    pub rdb: String,
    /// The alias (`*NONE`).
    pub alias: String,
    /// `RMTLOCNAME`: a host name or address, `*LOCAL`, `*ARDPGM`,
    /// `*LOOPBACK`.
    pub rmtloc: String,
    /// The location type: `*IP` or `*SNA`.
    pub loc_type: String,
    /// `TEXT`.
    pub text: String,
    /// `PORT`: `*DRDA` (446), a number or a service name.
    pub port: String,
    /// `RMTAUTMTH` preferred method.
    pub auth: String,
    /// `RMTAUTMTH` lower methods: `*ALWLOWER` or `*NOALWLOWER`.
    pub allow_lower: String,
    /// `SECCNN`: `*NONE`, `*SSL`, `*TLS`.
    pub seccnn: String,
    /// `ENCALG`: `*DES` or `*AES`.
    pub encalg: String,
    /// `ARDPGM`: `LIB/PGM`, empty for DRDA.
    pub ardpgm: String,
    /// `DEV`.
    pub dev: String,
    /// `LCLLOCNAME`.
    pub lcllocname: String,
    /// `RMTNETID`.
    pub rmtnetid: String,
    /// `MODE`.
    pub mode: String,
    /// `TNSPGM`.
    pub tnspgm: String,
}

impl RdbEntry {
    /// An entry with `ADDRDBDIRE`'s defaults.
    pub fn new(rdb: &str, rmtloc: &str, loc_type: &str) -> Self {
        Self {
            rdb: rdb.trim().to_ascii_uppercase(),
            alias: "*NONE".into(),
            rmtloc: rmtloc.trim().into(),
            loc_type: loc_type.into(),
            text: String::new(),
            port: "*DRDA".into(),
            auth: "*USRENCPWD".into(),
            allow_lower: "*ALWLOWER".into(),
            seccnn: "*NONE".into(),
            encalg: "*DES".into(),
            ardpgm: String::new(),
            dev: "*LOC".into(),
            lcllocname: "*LOC".into(),
            rmtnetid: "*LOC".into(),
            mode: "*NETATR".into(),
            tnspgm: "*DRDA".into(),
        }
    }

    /// The `*LOCAL` entry of a system.
    pub fn local(system: &str) -> Self {
        Self::new(system, "*LOCAL", "*IP")
    }

    /// Whether it names this system.
    pub fn is_local(&self) -> bool {
        self.rmtloc.eq_ignore_ascii_case("*LOCAL")
    }

    /// The TCP port to connect to.
    pub fn tcp_port(&self) -> u16 {
        match self.port.trim().to_ascii_uppercase().as_str() {
            "*DRDA" | "DRDA" | "" => 446,
            "DDM-SSL" | "DRDA-SSL" => 448,
            p => p.parse().unwrap_or(446),
        }
    }

    /// The mechanisms a requester tries for this entry, strongest
    /// first: the preferred one, then lower ones when allowed.
    pub fn mechanisms(&self) -> Vec<Mechanism> {
        let preferred = match self.auth.as_str() {
            "*ENCUSRPWD" => Mechanism::EUsrIdPwd,
            "*USRIDPWD" => Mechanism::UsrIdPwd,
            _ => Mechanism::UsrEncPwd,
        };
        let mut v = vec![preferred];
        if self.allow_lower != "*NOALWLOWER" {
            for m in Mechanism::ALL {
                if !v.contains(&m) {
                    v.push(m);
                }
            }
        }
        v
    }

    /// The line it stores as.
    pub fn encode(&self) -> String {
        let mut v = vec![kv("RDB", &self.rdb), kv("RMTLOCNAME", &self.rmtloc), kv("TYPE", &self.loc_type)];
        let opt = |k: &str, val: &str, dft: &str, v: &mut Vec<String>| {
            if val != dft {
                v.push(kv(k, val));
            }
        };
        opt("ALIAS", &self.alias, "*NONE", &mut v);
        opt("TEXT", &self.text, "", &mut v);
        opt("PORT", &self.port, "*DRDA", &mut v);
        opt("AUTH", &self.auth, "*USRENCPWD", &mut v);
        opt("LOWER", &self.allow_lower, "*ALWLOWER", &mut v);
        opt("SECCNN", &self.seccnn, "*NONE", &mut v);
        opt("ENCALG", &self.encalg, "*DES", &mut v);
        opt("ARDPGM", &self.ardpgm, "", &mut v);
        opt("DEV", &self.dev, "*LOC", &mut v);
        opt("LCLLOCNAME", &self.lcllocname, "*LOC", &mut v);
        opt("RMTNETID", &self.rmtnetid, "*LOC", &mut v);
        opt("MODE", &self.mode, "*NETATR", &mut v);
        opt("TNSPGM", &self.tnspgm, "*DRDA", &mut v);
        v.join(" ")
    }

    /// From its line.
    pub fn decode(line: &str) -> Option<Self> {
        Self::from_pairs(&parse(line))
    }

    fn from_pairs(p: &[(String, String)]) -> Option<Self> {
        let mut e = Self::new(get(p, "RDB")?, get(p, "RMTLOCNAME")?, get(p, "TYPE").unwrap_or("*SNA"));
        let set = |f: &mut String, k: &str| {
            if let Some(v) = get(p, k) {
                *f = v.to_string();
            }
        };
        set(&mut e.alias, "ALIAS");
        set(&mut e.text, "TEXT");
        set(&mut e.port, "PORT");
        set(&mut e.auth, "AUTH");
        set(&mut e.allow_lower, "LOWER");
        set(&mut e.seccnn, "SECCNN");
        set(&mut e.encalg, "ENCALG");
        set(&mut e.ardpgm, "ARDPGM");
        set(&mut e.dev, "DEV");
        set(&mut e.lcllocname, "LCLLOCNAME");
        set(&mut e.rmtnetid, "RMTNETID");
        set(&mut e.mode, "MODE");
        set(&mut e.tnspgm, "TNSPGM");
        Some(e)
    }
}

/// The directory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RdbDirectory {
    /// The entries, by name.
    pub entries: Vec<RdbEntry>,
}

/// Message: no such entry.
pub const NOT_FOUND: &str = "CPF9801";
/// Message: the entry exists already.
pub const EXISTS: &str = "CPF9870";

impl RdbDirectory {
    /// The directory of a new system: its `*LOCAL` entry.
    pub fn installed(system: &str) -> Self {
        Self {
            entries: vec![RdbEntry::local(system)],
        }
    }

    /// The stored text (one entry per line); empty text is the
    /// installed directory.
    pub fn decode(text: &str, system: &str) -> Self {
        if text.trim().is_empty() {
            return Self::installed(system);
        }
        let mut d = Self {
            entries: text.lines().filter_map(RdbEntry::decode).collect(),
        };
        d.sort();
        d
    }

    /// The text to store.
    pub fn encode(&self) -> String {
        self.entries.iter().map(RdbEntry::encode).collect::<Vec<_>>().join("\n")
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| a.rdb.cmp(&b.rdb));
    }

    /// An entry by name.
    pub fn find(&self, rdb: &str) -> Option<&RdbEntry> {
        let n = rdb.trim().to_ascii_uppercase();
        self.entries.iter().find(|e| e.rdb == n)
    }

    /// The `*LOCAL` entry.
    pub fn local(&self) -> Option<&RdbEntry> {
        self.entries.iter().find(|e| e.is_local())
    }

    /// `ADDRDBDIRE`.
    pub fn add(&mut self, e: RdbEntry) -> Result<(), Refused> {
        if self.find(&e.rdb).is_some() {
            return Err(Refused::new(EXISTS, format!("The directory has an entry for {} already.", e.rdb)));
        }
        if e.is_local() && self.local().is_some() {
            return Err(Refused::new(
                EXISTS,
                "Only one entry may be the *LOCAL relational database.",
            ));
        }
        self.entries.push(e);
        self.sort();
        Ok(())
    }

    /// `CHGRDBDIRE`: `f` changes the entry.
    pub fn change(&mut self, rdb: &str, f: impl FnOnce(&mut RdbEntry)) -> Result<(), Refused> {
        let n = rdb.trim().to_ascii_uppercase();
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.rdb == n)
            .ok_or_else(|| Refused::new(NOT_FOUND, format!("The directory has no entry for {n}.")))?;
        f(e);
        Ok(())
    }

    /// `RMVRDBDIRE`: a name, a generic name (`ABC*`), `*ALL` or
    /// `*ALLRMT`; how many went.
    pub fn remove(&mut self, rdb: &str) -> Result<usize, Refused> {
        let n = rdb.trim().to_ascii_uppercase();
        let before = self.entries.len();
        self.entries.retain(|e| !matches(&n, e));
        let gone = before - self.entries.len();
        if gone == 0 {
            return Err(Refused::new(NOT_FOUND, format!("The directory has no entry for {n}.")));
        }
        Ok(gone)
    }

    /// The entries `WRKRDBDIRE RDB(...)` lists.
    pub fn select(&self, rdb: &str) -> Vec<&RdbEntry> {
        let n = rdb.trim().to_ascii_uppercase();
        self.entries.iter().filter(|e| matches(&n, e)).collect()
    }
}

fn matches(pattern: &str, e: &RdbEntry) -> bool {
    match pattern {
        "*ALL" | "" => true,
        "*ALLRMT" => !e.is_local(),
        p => match p.strip_suffix('*') {
            Some(prefix) => e.rdb.starts_with(prefix),
            None => e.rdb == p,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_round_trip() {
        let mut d = RdbDirectory::installed("QS400");
        let mut e = RdbEntry::new("REMOTE1", "remote.example.com", "*IP");
        e.text = "a (remote) system".into();
        e.auth = "*ENCUSRPWD".into();
        d.add(e).unwrap();
        assert!(d.add(RdbEntry::new("REMOTE1", "x", "*IP")).is_err());
        let text = d.encode();
        let back = RdbDirectory::decode(&text, "QS400");
        assert_eq!(back, d);
        assert_eq!(back.find("remote1").unwrap().mechanisms()[0], Mechanism::EUsrIdPwd);
        assert_eq!(RdbDirectory::decode("", "SYS").local().unwrap().rdb, "SYS");
        let mut d = back;
        d.change("REMOTE1", |e| e.port = "8446".into()).unwrap();
        assert_eq!(d.find("REMOTE1").unwrap().tcp_port(), 8446);
        assert_eq!(d.select("*ALLRMT").len(), 1);
        assert_eq!(d.select("REM*").len(), 1);
        assert_eq!(d.remove("*ALLRMT").unwrap(), 1);
        assert!(d.remove("REMOTE1").is_err());
        assert_eq!(d.entries.len(), 1);
    }
}
