// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The algorithms offered, and `SSH_MSG_KEXINIT` with its negotiation
//! (RFC 4253 7.1), strict key exchange and `ext-info`.

use crate::error::{SshError, SshResult};
use crate::wire::{Reader, Writer, msg};

/// Hybrid ML-KEM-768 and X25519 with SHA-256.
pub const MLKEM768X25519: &str = "mlkem768x25519-sha256";
/// X25519 with SHA-256 (RFC 8731).
pub const CURVE25519: &str = "curve25519-sha256";
/// The pre-RFC name of the same.
pub const CURVE25519_LIBSSH: &str = "curve25519-sha256@libssh.org";
/// ECDH over P-256 (RFC 5656).
pub const ECDH_P256: &str = "ecdh-sha2-nistp256";

/// Key exchange methods, preferred first.
pub const KEX: &[&str] = &[MLKEM768X25519, CURVE25519, CURVE25519_LIBSSH, ECDH_P256];

/// AES-256-GCM (RFC 5647 as OpenSSH names it).
pub const AES256_GCM: &str = "aes256-gcm@openssh.com";
/// AES-128-GCM.
pub const AES128_GCM: &str = "aes128-gcm@openssh.com";

/// Ciphers, preferred first. Only AEAD ciphers: no MAC is ever used.
pub const CIPHERS: &[&str] = &[AES256_GCM, AES128_GCM];

/// MACs listed for clients that insist on a common one; with an AEAD
/// cipher the MAC is not negotiated.
pub const MACS: &[&str] = &["hmac-sha2-256-etm@openssh.com", "hmac-sha2-256"];

/// Compression: none.
pub const COMPRESSION: &[&str] = &["none"];

/// Strict key exchange, server side.
pub const STRICT_S: &str = "kex-strict-s-v00@openssh.com";
/// Strict key exchange, client side.
pub const STRICT_C: &str = "kex-strict-c-v00@openssh.com";
/// The client takes `SSH_MSG_EXT_INFO`.
pub const EXT_INFO_C: &str = "ext-info-c";

/// The key length of a cipher.
#[must_use]
pub fn key_len(cipher: &str) -> usize {
    if cipher == AES128_GCM { 16 } else { 32 }
}

/// One `SSH_MSG_KEXINIT`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KexInit {
    /// The random cookie.
    pub cookie: [u8; 16],
    /// Key exchange methods.
    pub kex: Vec<String>,
    /// Host key algorithms.
    pub host_key: Vec<String>,
    /// Ciphers client to server.
    pub cipher_cs: Vec<String>,
    /// Ciphers server to client.
    pub cipher_sc: Vec<String>,
    /// MACs client to server.
    pub mac_cs: Vec<String>,
    /// MACs server to client.
    pub mac_sc: Vec<String>,
    /// Compression client to server.
    pub comp_cs: Vec<String>,
    /// Compression server to client.
    pub comp_sc: Vec<String>,
    /// A guessed key exchange packet follows.
    pub first_follows: bool,
}

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

impl KexInit {
    /// What this side offers: `kex` with any pseudo-algorithms already
    /// added, `host_key` the signature algorithms of the keys held.
    #[must_use]
    pub fn offer(kex: &[&str], host_key: &[&str]) -> Self {
        let mut cookie = [0u8; 16];
        let _ = graviola::random::fill(&mut cookie);
        Self {
            cookie,
            kex: owned(kex),
            host_key: owned(host_key),
            cipher_cs: owned(CIPHERS),
            cipher_sc: owned(CIPHERS),
            mac_cs: owned(MACS),
            mac_sc: owned(MACS),
            comp_cs: owned(COMPRESSION),
            comp_sc: owned(COMPRESSION),
            first_follows: false,
        }
    }

    /// The message.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::msg(msg::KEXINIT);
        w.raw(&self.cookie);
        for list in [
            &self.kex,
            &self.host_key,
            &self.cipher_cs,
            &self.cipher_sc,
            &self.mac_cs,
            &self.mac_sc,
            &self.comp_cs,
            &self.comp_sc,
        ] {
            w.str(&list.join(","));
        }
        w.str("").str("").bool(self.first_follows).u32(0);
        w.finish()
    }

    /// Parse the message.
    pub fn decode(m: &[u8]) -> SshResult<Self> {
        let mut r = Reader::new(m);
        if r.u8()? != msg::KEXINIT {
            return Err(SshError::Protocol("expected KEXINIT".into()));
        }
        let mut k = Self::default();
        k.cookie.copy_from_slice(r.take(16)?);
        let mut list = || -> SshResult<Vec<String>> {
            Ok(r.names()?.into_iter().map(str::to_string).collect())
        };
        k.kex = list()?;
        k.host_key = list()?;
        k.cipher_cs = list()?;
        k.cipher_sc = list()?;
        k.mac_cs = list()?;
        k.mac_sc = list()?;
        k.comp_cs = list()?;
        k.comp_sc = list()?;
        list()?;
        list()?;
        k.first_follows = r.bool()?;
        Ok(k)
    }
}

/// What the two sides agreed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    /// The key exchange method.
    pub kex: String,
    /// The host key signature algorithm.
    pub host_key: String,
    /// Cipher client to server.
    pub cipher_cs: String,
    /// Cipher server to client.
    pub cipher_sc: String,
    /// Both sides asked for strict key exchange.
    pub strict: bool,
    /// The client takes `SSH_MSG_EXT_INFO`.
    pub ext_info: bool,
    /// The client's guessed packet is wrong and must be skipped.
    pub skip_guess: bool,
}

fn pick(client: &[String], server: &[String], what: &str) -> SshResult<String> {
    client
        .iter()
        .find(|c| server.contains(c))
        .cloned()
        .ok_or_else(|| SshError::Kex(format!("no {what} in common")))
}

/// Agree on the algorithms: the first of the client's the server has.
pub fn negotiate(client: &KexInit, server: &KexInit) -> SshResult<Negotiated> {
    let real = |v: &[String]| -> Vec<String> {
        v.iter()
            .filter(|a| !a.starts_with("kex-strict-") && !a.starts_with("ext-info-"))
            .cloned()
            .collect()
    };
    let kex = pick(&real(&client.kex), &real(&server.kex), "key exchange method")?;
    let host_key = pick(&client.host_key, &server.host_key, "host key algorithm")?;
    let cipher_cs = pick(&client.cipher_cs, &server.cipher_cs, "cipher")?;
    let cipher_sc = pick(&client.cipher_sc, &server.cipher_sc, "cipher")?;
    pick(&client.comp_cs, &server.comp_cs, "compression")?;
    pick(&client.comp_sc, &server.comp_sc, "compression")?;
    let has = |v: &[String], s: &str| v.iter().any(|a| a == s);
    let strict = has(&client.kex, STRICT_C) && has(&server.kex, STRICT_S);
    let skip_guess = client.first_follows
        && (client.kex.first() != Some(&kex) || client.host_key.first() != Some(&host_key));
    Ok(Negotiated {
        kex,
        host_key,
        cipher_cs,
        cipher_sc,
        strict,
        ext_info: has(&client.kex, EXT_INFO_C),
        skip_guess,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_order_wins_and_flags() {
        let mut c = KexInit::offer(&[CURVE25519, ECDH_P256, EXT_INFO_C, STRICT_C], &["ssh-ed25519"]);
        let s = KexInit::offer(&[MLKEM768X25519, ECDH_P256, CURVE25519, STRICT_S], &["rsa-sha2-256", "ssh-ed25519"]);
        let n = negotiate(&KexInit::decode(&c.encode()).unwrap(), &s).unwrap();
        assert_eq!(n.kex, CURVE25519);
        assert_eq!(n.host_key, "ssh-ed25519");
        assert!(n.strict && n.ext_info && !n.skip_guess);
        c.kex = vec!["diffie-hellman-group1-sha1".into()];
        assert!(negotiate(&c, &s).is_err());
    }
}
