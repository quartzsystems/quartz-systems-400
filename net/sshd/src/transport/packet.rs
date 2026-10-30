// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binary packet protocol (RFC 4253 6) in the clear and under
//! AES-GCM (RFC 5647 as OpenSSH uses it: the length is additional data,
//! the invocation counter is the IV's last eight bytes).

use std::io::Read;

use graviola::aead::AesGcm;

use crate::error::{SshError, SshResult};

/// The largest packet accepted, length field excluded.
pub const MAX_PACKET: usize = 256 * 1024 + 1024;

/// Keys for one direction.
#[derive(Clone)]
pub struct DirKeys {
    /// The cipher key (16 or 32 bytes).
    pub key: Vec<u8>,
    /// The initial IV (12 bytes).
    pub iv: [u8; 12],
}

impl DirKeys {
    /// From the derived material.
    #[must_use]
    pub fn new(key: Vec<u8>, iv: &[u8]) -> Self {
        let mut a = [0u8; 12];
        a.copy_from_slice(&iv[..12]);
        Self { key, iv: a }
    }
}

enum Protect {
    Clear,
    Gcm(Box<AesGcm>, [u8; 12]),
}

impl Protect {
    fn new(keys: Option<DirKeys>) -> Self {
        match keys {
            None => Self::Clear,
            Some(k) => Self::Gcm(Box::new(AesGcm::new(&k.key)), k.iv),
        }
    }
}

fn bump(iv: &mut [u8; 12]) {
    let mut c = [0u8; 8];
    c.copy_from_slice(&iv[4..]);
    let n = u64::from_be_bytes(c).wrapping_add(1);
    iv[4..].copy_from_slice(&n.to_be_bytes());
}

/// Reads and opens packets.
pub struct Opener {
    protect: Protect,
    /// The sequence number of the next packet.
    pub seq: u32,
}

impl Default for Opener {
    fn default() -> Self {
        Self {
            protect: Protect::Clear,
            seq: 0,
        }
    }
}

impl Opener {
    /// Switch keys after `NEWKEYS`; `reset` restarts the sequence (strict
    /// key exchange).
    pub fn rekey(&mut self, keys: DirKeys, reset: bool) {
        self.protect = Protect::new(Some(keys));
        if reset {
            self.seq = 0;
        }
    }

    /// The next payload.
    pub fn read(&mut self, r: &mut dyn Read) -> SshResult<Vec<u8>> {
        let mut len = [0u8; 4];
        read_exact(r, &mut len)?;
        let n = u32::from_be_bytes(len) as usize;
        if !(5..=MAX_PACKET).contains(&n) {
            return Err(SshError::Protocol(format!("packet length {n} out of range")));
        }
        let body = match &mut self.protect {
            Protect::Clear => {
                if !(n + 4).is_multiple_of(8) {
                    return Err(SshError::Protocol("packet not a multiple of the block".into()));
                }
                let mut body = vec![0u8; n];
                read_exact(r, &mut body)?;
                body
            }
            Protect::Gcm(aes, iv) => {
                if !n.is_multiple_of(16) {
                    return Err(SshError::Protocol("packet not a multiple of the block".into()));
                }
                let mut body = vec![0u8; n + 16];
                read_exact(r, &mut body)?;
                let tag = body.split_off(n);
                aes.decrypt(iv, &len, &mut body, &tag)
                    .map_err(|_| SshError::Integrity)?;
                bump(iv);
                body
            }
        };
        self.seq = self.seq.wrapping_add(1);
        let pad = usize::from(body[0]);
        if pad < 4 || pad + 1 > body.len() {
            return Err(SshError::Protocol("bad padding".into()));
        }
        Ok(body[1..body.len() - pad].to_vec())
    }
}

fn read_exact(r: &mut dyn Read, buf: &mut [u8]) -> SshResult<()> {
    r.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            SshError::Closed
        } else {
            SshError::Io(e)
        }
    })
}

/// Seals packets.
pub struct Sealer {
    protect: Protect,
    /// The sequence number of the next packet.
    pub seq: u32,
}

impl Default for Sealer {
    fn default() -> Self {
        Self {
            protect: Protect::Clear,
            seq: 0,
        }
    }
}

impl Sealer {
    /// Switch keys after sending `NEWKEYS`.
    pub fn rekey(&mut self, keys: DirKeys, reset: bool) {
        self.protect = Protect::new(Some(keys));
        if reset {
            self.seq = 0;
        }
    }

    /// The bytes to send for `payload`.
    #[must_use]
    pub fn seal(&mut self, payload: &[u8]) -> Vec<u8> {
        let gcm = matches!(self.protect, Protect::Gcm(..));
        let (block, counted) = if gcm { (16, 1) } else { (8, 5) };
        let mut pad = block - (counted + payload.len()) % block;
        if pad < 4 {
            pad += block;
        }
        let n = 1 + payload.len() + pad;
        let mut out = Vec::with_capacity(n + 4 + 16);
        out.extend_from_slice(&u32::try_from(n).unwrap_or(0).to_be_bytes());
        out.push(u8::try_from(pad).unwrap_or(4));
        out.extend_from_slice(payload);
        let mut padding = vec![0u8; pad];
        let _ = graviola::random::fill(&mut padding);
        out.extend_from_slice(&padding);
        if let Protect::Gcm(aes, iv) = &mut self.protect {
            let (len, body) = out.split_at_mut(4);
            let mut tag = [0u8; 16];
            aes.encrypt(iv, len, body, &mut tag);
            bump(iv);
            out.extend_from_slice(&tag);
        }
        self.seq = self.seq.wrapping_add(1);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_and_gcm_round_trip() {
        for keys in [None, Some(DirKeys::new(vec![7; 32], &[3; 12]))] {
            let mut s = Sealer::default();
            let mut o = Opener::default();
            if let Some(k) = keys {
                s.rekey(k.clone(), true);
                o.rekey(k, true);
            }
            let mut wire = Vec::new();
            for p in [&b"x"[..], &[9u8; 100], &[]] {
                wire.extend(s.seal(p));
            }
            let mut r = &wire[..];
            assert_eq!(o.read(&mut r).unwrap(), b"x");
            assert_eq!(o.read(&mut r).unwrap(), [9u8; 100]);
            assert_eq!(o.read(&mut r).unwrap(), b"");
            assert_eq!(o.seq, 3);
        }
    }

    #[test]
    fn tampering_is_caught() {
        let k = DirKeys::new(vec![1; 16], &[2; 12]);
        let mut s = Sealer::default();
        s.rekey(k.clone(), true);
        let mut o = Opener::default();
        o.rekey(k, true);
        let mut wire = s.seal(b"secret");
        wire[6] ^= 1;
        assert!(matches!(o.read(&mut &wire[..]), Err(SshError::Integrity)));
    }
}
