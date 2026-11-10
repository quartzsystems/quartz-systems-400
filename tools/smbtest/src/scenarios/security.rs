// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing on every dialect and encryption on the 3.x dialects.
//!
//! The client signs every non-guest session and checks the server's
//! signatures, so a round trip that succeeds is a signed one both ways.

use smb::Dialect;
use smb::connection::EncryptionMode;

use super::files::write_read_remove;
use super::{Lines, Scratch};
use crate::Target;
use crate::conn::{DIALECTS, Share, dialect_config, dialect_name};
use crate::error::check;

/// A signed round trip on each dialect; the algorithm each used.
pub async fn signing(t: &Target, sc: &Scratch) -> Lines {
    let mut lines = Vec::new();
    for d in DIALECTS {
        let s = Share::connect(t, dialect_config(t, d, EncryptionMode::Allowed)).await?;
        let info = s.info().await?;
        let algo = info
            .negotiation
            .signing_algo
            .unwrap_or_else(|| info.dialect.default_signing_algo());
        let path = sc.file(&format!("signed-{}.bin", dialect_name(d)));
        write_read_remove(&s, &path, b"signed both ways").await?;
        s.close().await?;
        lines.push(format!("{}: {algo:?}", dialect_name(d)));
    }
    Ok(lines)
}

/// Encryption required by the client on `d`; a round trip through it.
pub async fn encrypt(t: &Target, sc: &Scratch, d: Dialect) -> Lines {
    let s = Share::connect(t, dialect_config(t, d, EncryptionMode::Required)).await?;
    let info = s.info().await?;
    let cipher = info.negotiation.encryption_cipher;
    // 3.0 and 3.0.2 have no cipher context: AES-128-CCM is implied.
    check(cipher.is_some() || d < Dialect::Smb0311, || {
        "no cipher negotiated on 3.1.1".into()
    })?;
    let path = sc.file(&format!("sealed-{}.bin", dialect_name(d)));
    write_read_remove(&s, &path, b"sealed both ways").await?;
    s.close().await?;
    let offered: Vec<String> = smb::crypto::ENCRYPTING_ALGOS
        .iter()
        .map(|c| format!("{c:?}"))
        .collect();
    Ok(vec![
        format!("offered: {}", offered.join(" ")),
        format!(
            "cipher: {}",
            cipher.map_or_else(|| "Aes128Ccm (implied)".into(), |c| format!("{c:?}"))
        ),
    ])
}
