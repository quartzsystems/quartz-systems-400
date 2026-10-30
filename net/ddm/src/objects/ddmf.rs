// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A DDM file: a local name for a file on another system, reached
//! through record-level access. `CRTDDMF` makes one (defaults its
//! own), `CHGDDMF` changes it; `RMTLOCNAME(name *IP)` names the system
//! directly, `RMTLOCNAME(*RDB)` through a directory entry.

use super::text::{get, kv, parse};

/// The DDM file's description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdmFile {
    /// The file.
    pub file: String,
    /// Its library.
    pub library: String,
    /// `RMTFILE`: `LIB/FILE` or `FILE` (the remote library list), or a
    /// non-standard name.
    pub rmtfile: String,
    /// `RMTLOCNAME`: a host name or address, or `*RDB`.
    pub rmtloc: String,
    /// The location type: `*IP` or `*SNA`.
    pub loc_type: String,
    /// `RDB`, when `RMTLOCNAME(*RDB)`.
    pub rdb: String,
    /// `PORT`: 446 for `*DRDA`.
    pub port: u16,
    /// `ACCMTH`: `*RMTFILE`, `*COMBINED`, or the keyed/arrival and
    /// random/sequential pair.
    pub accmth: String,
    /// `SHARE`.
    pub share: bool,
    /// `PTCCNV`.
    pub ptccnv: bool,
    /// `LVLCHK`: `*RMTFILE` or `*NO`.
    pub lvlchk: String,
    /// `DEV`.
    pub dev: String,
    /// `LCLLOCNAME`.
    pub lcllocname: String,
    /// `MODE`.
    pub mode: String,
    /// `RMTNETID`.
    pub rmtnetid: String,
    /// `TEXT`.
    pub text: String,
}

impl DdmFile {
    /// A DDM file with `CRTDDMF`'s defaults.
    pub fn new(library: &str, file: &str, rmtfile: &str, rmtloc: &str, loc_type: &str) -> Self {
        Self {
            file: file.trim().to_ascii_uppercase(),
            library: library.trim().to_ascii_uppercase(),
            rmtfile: rmtfile.trim().into(),
            rmtloc: rmtloc.trim().into(),
            loc_type: loc_type.into(),
            rdb: String::new(),
            port: 446,
            accmth: "*RMTFILE".into(),
            share: false,
            ptccnv: false,
            lvlchk: "*RMTFILE".into(),
            dev: "*LOC".into(),
            lcllocname: "*LOC".into(),
            mode: "*NETATR".into(),
            rmtnetid: "*LOC".into(),
            text: String::new(),
        }
    }

    /// The remote library and file (`*LIBL` when unqualified); `None`
    /// for a non-standard name.
    pub fn remote_name(&self) -> Option<(String, String)> {
        let t = self.rmtfile.trim().to_ascii_uppercase();
        if t.starts_with('\'') || t.contains(' ') {
            return None;
        }
        Some(match t.split_once('/') {
            Some((l, f)) => (l.to_string(), f.to_string()),
            None => ("*LIBL".into(), t),
        })
    }

    /// Whether it reaches its system by TCP/IP.
    pub fn is_ip(&self) -> bool {
        self.loc_type == "*IP" || self.rmtloc == "*RDB"
    }

    /// The stored text.
    pub fn encode(&self) -> String {
        let yn = |b: bool| if b { "*YES" } else { "*NO" };
        [
            kv("FILE", &format!("{}/{}", self.library, self.file)),
            kv("RMTFILE", &self.rmtfile),
            kv("RMTLOCNAME", &self.rmtloc),
            kv("TYPE", &self.loc_type),
            kv("RDB", &self.rdb),
            kv("PORT", &self.port.to_string()),
            kv("ACCMTH", &self.accmth),
            kv("SHARE", yn(self.share)),
            kv("PTCCNV", yn(self.ptccnv)),
            kv("LVLCHK", &self.lvlchk),
            kv("DEV", &self.dev),
            kv("LCLLOCNAME", &self.lcllocname),
            kv("MODE", &self.mode),
            kv("RMTNETID", &self.rmtnetid),
            kv("TEXT", &self.text),
        ]
        .join(" ")
    }

    /// From the stored text.
    pub fn decode(text: &str) -> Option<Self> {
        Self::from_pairs(&parse(text))
    }

    /// Several DDM files stored one after another.
    pub fn encode_all(files: &[Self]) -> String {
        files.iter().map(Self::encode).collect::<Vec<_>>().join(" ")
    }

    /// Several DDM files from their stored text, each from its `FILE`
    /// keyword.
    pub fn decode_all(text: &str) -> Vec<Self> {
        let mut out = Vec::new();
        let mut cur: Vec<(String, String)> = Vec::new();
        for pair in parse(text) {
            if pair.0 == "FILE" && !cur.is_empty() {
                out.extend(Self::from_pairs(&cur));
                cur.clear();
            }
            cur.push(pair);
        }
        out.extend(Self::from_pairs(&cur));
        out
    }

    fn from_pairs(p: &[(String, String)]) -> Option<Self> {
        let (lib, file) = get(p, "FILE")?.split_once('/')?;
        let mut d = Self::new(lib, file, get(p, "RMTFILE")?, get(p, "RMTLOCNAME")?, get(p, "TYPE").unwrap_or("*SNA"));
        let s = |k: &str| get(p, k).unwrap_or_default().to_string();
        d.rdb = s("RDB");
        d.port = get(p, "PORT").and_then(|v| v.parse().ok()).unwrap_or(446);
        d.accmth = s("ACCMTH");
        d.share = get(p, "SHARE") == Some("*YES");
        d.ptccnv = get(p, "PTCCNV") == Some("*YES");
        d.lvlchk = s("LVLCHK");
        d.dev = s("DEV");
        d.lcllocname = s("LCLLOCNAME");
        d.mode = s("MODE");
        d.rmtnetid = s("RMTNETID");
        d.text = s("TEXT");
        Some(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut d = DdmFile::new("QSTEST", "RCUST", "QSTEST/CUST", "other.example.com", "*IP");
        d.text = "Customers on the other system".into();
        let back = DdmFile::decode(&d.encode()).unwrap();
        let two = DdmFile::decode_all(&DdmFile::encode_all(&[d.clone(), d.clone()]));
        assert_eq!(two.len(), 2);
        assert_eq!(back, d);
        assert_eq!(back.remote_name(), Some(("QSTEST".into(), "CUST".into())));
        assert!(back.is_ip());
    }
}
