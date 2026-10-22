// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The audit entry types: IBM's two-letter codes, the action auditing
//! value (`QAUDLVL`) that selects each, and whether QS/400 writes it.

/// An audit entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Authority failure.
    Af,
    /// Auditing change.
    Ad,
    /// Attribute change.
    Au,
    /// Authority change.
    Ca,
    /// Object created.
    Co,
    /// User profile created, changed or restored.
    Cp,
    /// Object deleted.
    Do,
    /// Service tools password reset (DST).
    Ds,
    /// Generic record: exit programs.
    Gr,
    /// Job description `USER` changed.
    Jd,
    /// Job start, end, hold, release, change, submit.
    Js,
    /// Object moved or renamed.
    Om,
    /// Object restored.
    Or,
    /// Object owner changed.
    Ow,
    /// Program adopting its owner changed.
    Pa,
    /// Primary group changed.
    Pg,
    /// Profile swap.
    Ps,
    /// Password or user not valid.
    Pw,
    /// Authority changed by a restore.
    Ra,
    /// Owner changed by a restore.
    Ro,
    /// Adopting program restored.
    Rp,
    /// Authority restored for a profile.
    Ru,
    /// Primary group changed by a restore.
    Rz,
    /// Subsystem routing changed.
    Se,
    /// Spooled file action.
    Sf,
    /// Server security user information.
    So,
    /// Service tools used.
    St,
    /// System value changed.
    Sv,
    /// Server session (network).
    Va,
    /// Validation list.
    Vo,
    /// Network password error.
    Vp,
    /// Network profile change.
    Vu,
    /// Object changed (object auditing).
    Zc,
    /// Object read (object auditing).
    Zr,
    /// Network authentication.
    X0,
    /// Identity token.
    X1,
    /// Query manager profile change.
    X2,
}

/// Every kind, in code order.
pub const ALL: [Kind; 37] = [
    Kind::Ad,
    Kind::Af,
    Kind::Au,
    Kind::Ca,
    Kind::Co,
    Kind::Cp,
    Kind::Do,
    Kind::Ds,
    Kind::Gr,
    Kind::Jd,
    Kind::Js,
    Kind::Om,
    Kind::Or,
    Kind::Ow,
    Kind::Pa,
    Kind::Pg,
    Kind::Ps,
    Kind::Pw,
    Kind::Ra,
    Kind::Ro,
    Kind::Rp,
    Kind::Ru,
    Kind::Rz,
    Kind::Se,
    Kind::Sf,
    Kind::So,
    Kind::St,
    Kind::Sv,
    Kind::Va,
    Kind::Vo,
    Kind::Vp,
    Kind::Vu,
    Kind::X0,
    Kind::X1,
    Kind::X2,
    Kind::Zc,
    Kind::Zr,
];

impl Kind {
    /// The two letters.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Af => "AF",
            Self::Ad => "AD",
            Self::Au => "AU",
            Self::Ca => "CA",
            Self::Co => "CO",
            Self::Cp => "CP",
            Self::Do => "DO",
            Self::Ds => "DS",
            Self::Gr => "GR",
            Self::Jd => "JD",
            Self::Js => "JS",
            Self::Om => "OM",
            Self::Or => "OR",
            Self::Ow => "OW",
            Self::Pa => "PA",
            Self::Pg => "PG",
            Self::Ps => "PS",
            Self::Pw => "PW",
            Self::Ra => "RA",
            Self::Ro => "RO",
            Self::Rp => "RP",
            Self::Ru => "RU",
            Self::Rz => "RZ",
            Self::Se => "SE",
            Self::Sf => "SF",
            Self::So => "SO",
            Self::St => "ST",
            Self::Sv => "SV",
            Self::Va => "VA",
            Self::Vo => "VO",
            Self::Vp => "VP",
            Self::Vu => "VU",
            Self::Zc => "ZC",
            Self::Zr => "ZR",
            Self::X0 => "X0",
            Self::X1 => "X1",
            Self::X2 => "X2",
        }
    }

    /// By its two letters.
    #[must_use]
    pub fn by_code(c: &str) -> Option<Self> {
        ALL.into_iter().find(|k| k.code().eq_ignore_ascii_case(c))
    }

    /// The action auditing value that selects it; `None` for object
    /// auditing (`ZC`, `ZR`), which `OBJAUD` selects.
    #[must_use]
    pub fn category(self) -> Option<&'static str> {
        Some(match self {
            Self::Af | Self::Pw => "*AUTFAIL",
            Self::Co => "*CREATE",
            Self::Do => "*DELETE",
            Self::Ad | Self::Cp | Self::Ds | Self::Sv | Self::Se | Self::Au => "*SECCFG",
            Self::Ca | Self::Ow | Self::Pg | Self::Pa | Self::Jd | Self::Gr => "*SECRUN",
            Self::Ps | Self::X1 => "*SECVFY",
            Self::Js => "*JOBBAS",
            Self::Om => "*OBJMGT",
            Self::Or | Self::Ra | Self::Ro | Self::Rp | Self::Ru | Self::Rz => "*SAVRST",
            Self::Sf => "*SPLFDTA",
            Self::St => "*SERVICE",
            Self::So | Self::Vo => "*SECVLDL",
            Self::Va | Self::Vp | Self::Vu | Self::X2 => "*NETCMN",
            Self::X0 => "*SECNAS",
            Self::Zc | Self::Zr => return None,
        })
    }

    /// Whether QS/400 writes it; the rest wait for restore (Phase 11) or
    /// for the network.
    #[must_use]
    pub fn implemented(self) -> bool {
        !matches!(
            self,
            Self::Or
                | Self::Ra
                | Self::Ro
                | Self::Rp
                | Self::Ru
                | Self::Rz
                | Self::So
                | Self::Va
                | Self::Vo
                | Self::Vp
                | Self::Vu
                | Self::X0
                | Self::X1
                | Self::X2
        )
    }
}

/// The `QAUDLVL` values that stand for several (`*SECURITY` is every
/// `*SECxxx`, `*JOBDTA` is `*JOBBAS` and `*JOBCHGUSR`).
#[must_use]
pub fn expand(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for v in values {
        let v = v.to_ascii_uppercase();
        match v.as_str() {
            "*SECURITY" => out.extend(
                [
                    "*SECCFG",
                    "*SECDIRSRV",
                    "*SECIPC",
                    "*SECNAS",
                    "*SECRUN",
                    "*SECSCKD",
                    "*SECVFY",
                    "*SECVLDL",
                ]
                .map(String::from),
            ),
            "*JOBDTA" => out.extend(["*JOBBAS", "*JOBCHGUSR"].map(String::from)),
            "*NETCMN" => out
                .extend(["*NETBAS", "*NETCLU", "*NETFAIL", "*NETSCK", "*NETCMN"].map(String::from)),
            _ => out.push(v),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for k in ALL {
            assert_eq!(Kind::by_code(k.code()), Some(k));
        }
        assert_eq!(Kind::Af.category(), Some("*AUTFAIL"));
        assert_eq!(Kind::Zr.category(), None);
        assert!(!Kind::Or.implemented());
        assert!(expand(&["*SECURITY".into()]).contains(&"*SECRUN".to_string()));
    }
}
