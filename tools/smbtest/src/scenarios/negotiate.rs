// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Negotiating each dialect alone, and the multi-protocol (SMB1 first) form.

use smb::Dialect;
use smb::connection::EncryptionMode;

use super::Lines;
use crate::conn::{Share, base_config, dialect_config, dialect_name};
use crate::error::check;
use crate::Target;

/// Offers only `d`; the server must choose it and serve a tree.
pub async fn dialect(t: &Target, d: Dialect) -> Lines {
    let s = Share::connect(t, dialect_config(t, d, EncryptionMode::Allowed)).await?;
    let info = s.info().await?;
    let got = info.negotiation.dialect_rev;
    check(got == d, || format!("offered {} only, server chose {}", dialect_name(d), dialect_name(got)))?;
    let lines = describe(&info);
    s.close().await?;
    Ok(lines)
}

/// The negotiate a Windows client sends first: SMB1 with the SMB2 dialect
/// strings, upgraded to SMB2. The highest dialect must win.
pub async fn multiprotocol(t: &Target) -> Lines {
    let config = smb::ConnectionConfig { smb2_only_negotiate: false, ..base_config(t) };
    let s = Share::connect(t, config).await?;
    let info = s.info().await?;
    check(info.negotiation.dialect_rev == Dialect::Smb0311, || {
        format!("expected 3.1.1 after the upgrade, got {}", dialect_name(info.negotiation.dialect_rev))
    })?;
    let lines = describe(&info);
    s.close().await?;
    Ok(lines)
}

/// The negotiated properties worth printing.
pub fn describe(info: &smb::connection::connection_info::ConnectionInfo) -> Vec<String> {
    let n = &info.negotiation;
    let signing = n.signing_algo.unwrap_or_else(|| info.dialect.default_signing_algo());
    let mut v = vec![
        format!("dialect: {}", dialect_name(n.dialect_rev)),
        format!("signing: {signing:?}"),
        format!("max read/write/transact: {}/{}/{}", n.max_read_size, n.max_write_size, n.max_transact_size),
    ];
    if let Some(c) = n.encryption_cipher {
        v.push(format!("cipher: {c:?}"));
    }
    v
}
