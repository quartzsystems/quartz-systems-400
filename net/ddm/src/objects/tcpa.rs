// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM server's TCP/IP attributes (`CHGDDMTCPA`): whether
//! `STRTCP` starts it, the lowest authentication method it accepts
//! (`PWDRQD`) and the lowest encryption algorithm (`ENCALG`).

use super::text::{get, kv, parse};
use crate::sec::Mechanism;

/// The attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdmTcpAttrs {
    /// `AUTOSTART`.
    pub autostart: bool,
    /// `PWDRQD`: `*USRID`, `*VLDONLY`, `*USRIDPWD`, `*USRENCPWD`,
    /// `*ENCUSRPWD`, `*KERBEROS`.
    pub pwdrqd: String,
    /// `ENCALG`: `*DES` or `*AES`.
    pub encalg: String,
}

impl Default for DdmTcpAttrs {
    fn default() -> Self {
        Self {
            autostart: true,
            pwdrqd: "*USRENCPWD".into(),
            encalg: "*DES".into(),
        }
    }
}

impl DdmTcpAttrs {
    /// The value of a `PWDRQD` keyword as stored (`*YES` is the old
    /// name of `*USRIDPWD`, `*NO` of `*USRID`, `*ENCRYPTED` of
    /// `*USRENCPWD`).
    pub fn normal_pwdrqd(v: &str) -> String {
        match v.trim().to_ascii_uppercase().as_str() {
            "*YES" => "*USRIDPWD".into(),
            "*NO" => "*USRID".into(),
            "*ENCRYPTED" => "*USRENCPWD".into(),
            other => other.to_string(),
        }
    }

    /// The mechanisms the server offers, in the order it offers them.
    /// Mechanisms that prove no password (`*USRID`, `*VLDONLY`) and
    /// Kerberos are not built; they offer what `*USRIDPWD` does.
    pub fn mechanisms(&self) -> Vec<Mechanism> {
        match self.pwdrqd.as_str() {
            "*ENCUSRPWD" => vec![Mechanism::EUsrIdPwd],
            "*USRENCPWD" => vec![
                Mechanism::UsrSsbPwd,
                Mechanism::UsrEncPwd,
                Mechanism::EUsrIdPwd,
            ],
            _ => vec![
                Mechanism::UsrSsbPwd,
                Mechanism::UsrEncPwd,
                Mechanism::EUsrIdPwd,
                Mechanism::UsrSbsPwd,
                Mechanism::UsrIdPwd,
            ],
        }
    }

    /// The stored text.
    pub fn encode(&self) -> String {
        [
            kv("AUTOSTART", if self.autostart { "*YES" } else { "*NO" }),
            kv("PWDRQD", &self.pwdrqd),
            kv("ENCALG", &self.encalg),
        ]
        .join(" ")
    }

    /// From the stored text (the defaults where it is silent).
    pub fn decode(text: &str) -> Self {
        let p = parse(text);
        let d = Self::default();
        Self {
            autostart: get(&p, "AUTOSTART").map_or(d.autostart, |v| v == "*YES"),
            pwdrqd: get(&p, "PWDRQD").map_or(d.pwdrqd, Self::normal_pwdrqd),
            encalg: get(&p, "ENCALG").map_or(d.encalg, str::to_string),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_mechanisms() {
        let mut a = DdmTcpAttrs::decode("");
        assert_eq!(a, DdmTcpAttrs::default());
        a.pwdrqd = DdmTcpAttrs::normal_pwdrqd("*encusrpwd");
        a.autostart = false;
        assert_eq!(DdmTcpAttrs::decode(&a.encode()), a);
        assert_eq!(a.mechanisms(), vec![Mechanism::EUsrIdPwd]);
        assert!(!DdmTcpAttrs::default().mechanisms().contains(&Mechanism::UsrIdPwd));
    }
}
