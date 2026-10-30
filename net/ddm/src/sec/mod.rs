// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security mechanisms of `ACCSEC`/`SECCHK`:
//!
//! - `USRIDPWD` (3): user ID and password in the clear (EBCDIC).
//! - `USRSBSPWD` (6) and `USRSSBPWD` (8): the host servers' password
//!   substitutes (DES; SHA-1 or SHA-512) over seeds carried in the
//!   `SECTKN` of `ACCSEC` (the requester's) and `ACCSECRD` (the
//!   server's).
//! - `USRENCPWD` (7) and `EUSRIDPWD` (9): a Diffie-Hellman exchange of
//!   256-bit public keys in the two `SECTKN`s; the DES key is bytes 12
//!   to 19 of the shared secret, the CBC vector the user ID (7) or bytes
//!   12 to 19 of the server's public key (9), PKCS #5 padding.

pub mod bignum;
pub mod des;

use crate::Error;
use bignum::Num;

/// The prime of the 256-bit exchange (DES).
pub const DES_PRIME: [u8; 32] = [
    0xC6, 0x21, 0x12, 0xD7, 0x3E, 0xE6, 0x13, 0xF0, 0x94, 0x7A, 0xB3, 0x1F, 0x0F, 0x68, 0x46, 0xA1,
    0xBF, 0xF5, 0xB3, 0xA4, 0xCA, 0x0D, 0x60, 0xBC, 0x1E, 0x4C, 0x7A, 0x0D, 0x8C, 0x16, 0xB3, 0xE3,
];

/// The generator of the 256-bit exchange.
pub const DES_GENERATOR: [u8; 32] = [
    0x46, 0x90, 0xFA, 0x1F, 0x7B, 0x9E, 0x1D, 0x44, 0x42, 0xC8, 0x6C, 0x91, 0x14, 0x60, 0x3F, 0xDE,
    0xCF, 0x07, 0x1E, 0xDC, 0xEC, 0x5F, 0x62, 0x6E, 0x21, 0xE2, 0x56, 0xAE, 0xD9, 0xEA, 0x34, 0xE4,
];

/// A security mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mechanism {
    /// User ID and password.
    UsrIdPwd,
    /// User ID and DES password substitute.
    UsrSbsPwd,
    /// User ID and encrypted password.
    UsrEncPwd,
    /// User ID and SHA password substitute.
    UsrSsbPwd,
    /// Encrypted user ID and password.
    EUsrIdPwd,
}

impl Mechanism {
    /// Every mechanism QS/400 speaks, strongest first.
    pub const ALL: [Self; 5] = [
        Self::EUsrIdPwd,
        Self::UsrSsbPwd,
        Self::UsrEncPwd,
        Self::UsrSbsPwd,
        Self::UsrIdPwd,
    ];

    /// Its `SECMEC` value.
    pub fn code(self) -> u16 {
        use crate::cp::secmec as s;
        match self {
            Self::UsrIdPwd => s::USRIDPWD,
            Self::UsrSbsPwd => s::USRSBSPWD,
            Self::UsrEncPwd => s::USRENCPWD,
            Self::UsrSsbPwd => s::USRSSBPWD,
            Self::EUsrIdPwd => s::EUSRIDPWD,
        }
    }

    /// From a `SECMEC` value.
    pub fn from_code(c: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == c)
    }

    /// Whether `ACCSEC` carries a public key.
    pub fn uses_dh(self) -> bool {
        matches!(self, Self::UsrEncPwd | Self::EUsrIdPwd)
    }

    /// Whether `ACCSEC` carries a seed.
    pub fn uses_seed(self) -> bool {
        matches!(self, Self::UsrSbsPwd | Self::UsrSsbPwd)
    }
}

/// Random bytes from the system.
pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::fill(&mut b).is_err() {
        // No entropy source: fall back on the clock, never on zeros.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos());
        for (i, x) in b.iter_mut().enumerate() {
            let [lo, ..] = (t >> ((i % 16) * 8)).to_le_bytes();
            *x = lo ^ (i.to_le_bytes()[0]).wrapping_mul(151);
        }
    }
    b
}

/// One side of the 256-bit Diffie-Hellman exchange.
#[derive(Clone)]
pub struct KeyPair {
    private: Num,
    /// The public key, 32 bytes.
    pub public: [u8; 32],
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair").field("public", &self.public).finish_non_exhaustive()
    }
}

impl KeyPair {
    /// A fresh pair.
    pub fn generate() -> Self {
        Self::from_private(&random::<32>())
    }

    /// The pair of a given private value (tests).
    pub fn from_private(private: &[u8]) -> Self {
        let p = Num::from_be(&DES_PRIME);
        let private = Num::from_be(private);
        let public = Num::from_be(&DES_GENERATOR).pow_mod(&private, &p);
        let mut out = [0u8; 32];
        out.copy_from_slice(&public.to_be(32));
        Self {
            private,
            public: out,
        }
    }

    /// The shared secret with the partner's public key.
    pub fn shared(&self, partner: &[u8]) -> Result<[u8; 32], Error> {
        if partner.len() != 32 {
            return Err(Error::Invalid(format!(
                "a public key of {} bytes",
                partner.len()
            )));
        }
        let p = Num::from_be(&DES_PRIME);
        let s = Num::from_be(partner).pow_mod(&self.private, &p);
        let mut out = [0u8; 32];
        out.copy_from_slice(&s.to_be(32));
        Ok(out)
    }
}

/// The DES key of a shared secret: its middle eight bytes.
pub fn des_key(shared: &[u8; 32]) -> [u8; 8] {
    let mut k = [0u8; 8];
    k.copy_from_slice(&shared[12..20]);
    k
}

/// The CBC vector of `USRENCPWD`: the user ID, zero padded or cut to
/// eight bytes.
pub fn usrencpwd_iv(user_id: &[u8]) -> [u8; 8] {
    let mut v = [0u8; 8];
    for (d, s) in v.iter_mut().zip(user_id) {
        *d = *s;
    }
    v
}

/// The CBC vector of `EUSRIDPWD`: bytes 12 to 19 of the server's public
/// key.
pub fn eusridpwd_iv(server_public: &[u8]) -> [u8; 8] {
    let mut v = [0u8; 8];
    if server_public.len() >= 20 {
        v.copy_from_slice(&server_public[12..20]);
    }
    v
}

/// A user ID as `SECCHK` carries it: upper case EBCDIC, blank padded
/// to ten bytes.
pub fn user_id(user: &str) -> Vec<u8> {
    crate::wire::ebcdic_fixed(&user.trim().to_ascii_uppercase(), 10)
}

/// A password as `SECCHK` carries it: as typed, in CCSID 500.
pub fn password_bytes(password: &str) -> Vec<u8> {
    ccsid::conv::from_str(500, password).unwrap_or_else(|_| crate::wire::ebcdic(password))
}

/// A password from its `SECCHK` bytes.
pub fn password_text(b: &[u8]) -> String {
    ccsid::conv::to_string(500, b).unwrap_or_else(|_| crate::wire::text(b))
}

/// What the requester sends in `SECCHK` for a mechanism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecchkData {
    /// `USRID`, when sent in the clear.
    pub usrid: Option<Vec<u8>>,
    /// `PASSWORD` (clear or substitute).
    pub password: Option<Vec<u8>>,
    /// `SECTKN`s, in order (encrypted user ID, then password).
    pub sectkn: Vec<Vec<u8>>,
}

/// The requester's state between `ACCSEC` and `SECCHK`.
#[derive(Clone, Debug)]
pub struct Requester {
    /// The mechanism.
    pub mech: Mechanism,
    /// The key pair (DH mechanisms).
    pub keys: Option<KeyPair>,
    /// The requester's seed (substitute mechanisms).
    pub seed: [u8; 8],
}

impl Requester {
    /// Start a mechanism.
    pub fn new(mech: Mechanism) -> Self {
        Self {
            mech,
            keys: mech.uses_dh().then(KeyPair::generate),
            seed: random::<8>(),
        }
    }

    /// The `SECTKN` for `ACCSEC`, if the mechanism sends one.
    pub fn accsec_token(&self) -> Option<Vec<u8>> {
        if let Some(k) = &self.keys {
            Some(k.public.to_vec())
        } else if self.mech.uses_seed() {
            Some(self.seed.to_vec())
        } else {
            None
        }
    }

    /// `SECCHK`'s data for `user` and `password`, given the server's
    /// `SECTKN` from `ACCSECRD` (public key or seed).
    pub fn secchk(
        &self,
        user: &str,
        password: &str,
        server_token: Option<&[u8]>,
        sha512: bool,
    ) -> Result<SecchkData, Error> {
        let uid = user_id(user);
        let pwd = password_bytes(password);
        let token = || server_token.ok_or(Error::Missing("SECTKN"));
        Ok(match self.mech {
            Mechanism::UsrIdPwd => SecchkData {
                usrid: Some(uid),
                password: Some(pwd),
                sectkn: Vec::new(),
            },
            Mechanism::UsrSbsPwd | Mechanism::UsrSsbPwd => {
                let t = token()?;
                let server: [u8; 8] = t
                    .try_into()
                    .map_err(|_| Error::Invalid("a server seed of other than 8 bytes".into()))?;
                let level = match (self.mech, sha512) {
                    (Mechanism::UsrSbsPwd, _) => hostsvr::crypt::Level::Des,
                    (_, false) => hostsvr::crypt::Level::Sha1,
                    (_, true) => hostsvr::crypt::Level::Sha512,
                };
                let seeds = hostsvr::crypt::Seeds {
                    client: self.seed,
                    server,
                };
                let sub = hostsvr::crypt::substitute(level, user, password, &seeds)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                SecchkData {
                    usrid: Some(uid),
                    password: Some(sub),
                    sectkn: Vec::new(),
                }
            }
            Mechanism::UsrEncPwd => {
                let t = token()?;
                let keys = self.keys.as_ref().ok_or(Error::Missing("key pair"))?;
                let key = des_key(&keys.shared(t)?);
                let enc = des::cbc_encrypt(&key, &usrencpwd_iv(&uid), &pwd);
                SecchkData {
                    usrid: Some(uid),
                    password: None,
                    sectkn: vec![enc],
                }
            }
            Mechanism::EUsrIdPwd => {
                let t = token()?;
                let keys = self.keys.as_ref().ok_or(Error::Missing("key pair"))?;
                let key = des_key(&keys.shared(t)?);
                let iv = eusridpwd_iv(t);
                SecchkData {
                    usrid: None,
                    password: None,
                    sectkn: vec![
                        des::cbc_encrypt(&key, &iv, &uid),
                        des::cbc_encrypt(&key, &iv, &pwd),
                    ],
                }
            }
        })
    }
}

/// The server's state between `ACCSECRD` and `SECCHK`.
#[derive(Clone, Debug)]
pub struct Server {
    /// The mechanism.
    pub mech: Mechanism,
    /// The key pair (DH mechanisms).
    pub keys: Option<KeyPair>,
    /// The server's seed (substitute mechanisms).
    pub seed: [u8; 8],
    /// The requester's `ACCSEC` token.
    pub requester_token: Option<Vec<u8>>,
}

/// What a `SECCHK` proved, for the authenticator to check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    /// The password itself.
    Password(String),
    /// A substitute over the seeds.
    Substitute {
        /// The substitute.
        substitute: Vec<u8>,
        /// The requester's seed.
        client_seed: [u8; 8],
        /// The server's seed.
        server_seed: [u8; 8],
    },
}

impl Server {
    /// Answer `ACCSEC` for `mech` with the requester's token.
    pub fn new(mech: Mechanism, requester_token: Option<Vec<u8>>) -> Self {
        Self {
            mech,
            keys: mech.uses_dh().then(KeyPair::generate),
            seed: random::<8>(),
            requester_token,
        }
    }

    /// The `SECTKN` for `ACCSECRD`.
    pub fn accsecrd_token(&self) -> Option<Vec<u8>> {
        if let Some(k) = &self.keys {
            Some(k.public.to_vec())
        } else if self.mech.uses_seed() {
            Some(self.seed.to_vec())
        } else {
            None
        }
    }

    /// The user and what proves it from `SECCHK`'s parameters.
    pub fn open(
        &self,
        usrid: Option<&[u8]>,
        password: Option<&[u8]>,
        sectkn: &[&[u8]],
    ) -> Result<(String, Proof), u16> {
        use crate::cp::secchkcd as c;
        let text = |b: &[u8]| crate::wire::text(b);
        match self.mech {
            Mechanism::UsrIdPwd => {
                let u = usrid.ok_or(c::USERID_MISSING)?;
                let p = password.ok_or(c::PASSWORD_MISSING)?;
                Ok((text(u), Proof::Password(password_text(p))))
            }
            Mechanism::UsrSbsPwd | Mechanism::UsrSsbPwd => {
                let u = usrid.ok_or(c::USERID_MISSING)?;
                let p = password.ok_or(c::PASSWORD_MISSING)?;
                let client_seed: [u8; 8] = self
                    .requester_token
                    .as_deref()
                    .and_then(|t| t.try_into().ok())
                    .ok_or(c::SECTKN_MISSING)?;
                Ok((
                    text(u),
                    Proof::Substitute {
                        substitute: p.to_vec(),
                        client_seed,
                        server_seed: self.seed,
                    },
                ))
            }
            Mechanism::UsrEncPwd | Mechanism::EUsrIdPwd => {
                let keys = self.keys.as_ref().ok_or(c::SECTKN_MISSING)?;
                let theirs = self.requester_token.as_deref().ok_or(c::SECTKN_MISSING)?;
                let key = des_key(&keys.shared(theirs).map_err(|_| c::SECTKN_MISSING)?);
                if self.mech == Mechanism::UsrEncPwd {
                    let u = usrid.ok_or(c::USERID_MISSING)?;
                    let enc = sectkn.first().ok_or(c::PASSWORD_MISSING)?;
                    let p = des::cbc_decrypt(&key, &usrencpwd_iv(u), enc)
                        .ok_or(c::PASSWORD_INVALID)?;
                    Ok((text(u), Proof::Password(password_text(&p))))
                } else {
                    let iv = eusridpwd_iv(&keys.public);
                    let [eu, ep, ..] = sectkn else {
                        return Err(c::SECTKN_MISSING);
                    };
                    let u = des::cbc_decrypt(&key, &iv, eu).ok_or(c::USERID_INVALID)?;
                    let p = des::cbc_decrypt(&key, &iv, ep).ok_or(c::PASSWORD_INVALID)?;
                    Ok((text(&u), Proof::Password(password_text(&p))))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_agrees() {
        let a = KeyPair::from_private(&[3u8; 32]);
        let b = KeyPair::from_private(&[0x55u8; 32]);
        assert_eq!(a.shared(&b.public).unwrap(), b.shared(&a.public).unwrap());
    }

    #[test]
    fn every_mechanism_round_trips() {
        for mech in Mechanism::ALL {
            let r = Requester::new(mech);
            let s = Server::new(mech, r.accsec_token());
            let d = r
                .secchk("qsuser", "Secret9", s.accsecrd_token().as_deref(), false)
                .unwrap();
            let tk: Vec<&[u8]> = d.sectkn.iter().map(Vec::as_slice).collect();
            let (user, proof) = s
                .open(d.usrid.as_deref(), d.password.as_deref(), &tk)
                .unwrap();
            assert_eq!(user, "QSUSER");
            match proof {
                Proof::Password(p) => assert_eq!(p, "Secret9"),
                Proof::Substitute { substitute, .. } => assert!(!substitute.is_empty()),
            }
        }
    }
}
