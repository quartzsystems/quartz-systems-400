// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign-on (`NTLMv2` over `SPNEGO`) and the share list through `srvsvc`.

use super::Lines;
use crate::conn::{Share, base_config};
use crate::error::check;
use crate::{Error, Target};

/// `STATUS_LOGON_FAILURE`.
pub const LOGON_FAILURE: u32 = 0xC000_006D;

/// Why `share-enum` is skipped against a server without NDR64.
pub const NDR64_ONLY: &str = "smb 0.12.1 binds srvsvc offering only NDR64 (and bind time feature negotiation); \
the server accepts only NDR 2.0, which Windows, macOS and Samba clients offer; covered by tests/interop.rs (smbclient -L)";

/// The right password signs on and connects the share.
pub async fn good(t: &Target) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    s.close().await?;
    Ok(vec![format!("share: {}", t.share)])
}

/// One wrong password: `STATUS_LOGON_FAILURE`, nothing else.
pub async fn wrong_password(t: &Target) -> Lines {
    let wrong = format!("{}x", t.password);
    refused(Share::connect_as(t, base_config(t), &t.user, &wrong, &t.share).await)
}

/// A user that does not exist: the same status as a wrong password.
pub async fn unknown_user(t: &Target) -> Lines {
    refused(Share::connect_as(t, base_config(t), "QSNOSUCHUSR", &t.password, &t.share).await)
}

fn refused(r: Result<Share, Error>) -> Lines {
    match r {
        Ok(_) => Err(Error::Check("signed on".into())),
        Err(e) => {
            let status = e.status();
            check(status == Some(LOGON_FAILURE), || format!("expected 0x{LOGON_FAILURE:08X}, got {e}"))?;
            Ok(vec![format!("status: 0x{LOGON_FAILURE:08X}")])
        }
    }
}

/// `NetShareEnumAll` over `IPC$`: the file share (and the print share, when
/// given) must be listed.
pub async fn share_enum(t: &Target) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    s.client.ipc_connect(&t.host, &t.user, t.password.clone()).await?;
    let shares = match s.client.list_shares(&t.host).await {
        Ok(v) => v,
        // smb 0.12.1 binds srvsvc with NDR64 only; a server that speaks only
        // NDR 2.0 (the vendored one) refuses the syntax.
        Err(e) if e.to_string().contains("ProposedTransferSyntaxesNotSupported") => {
            s.close().await?;
            return Err(Error::Inexpressible(NDR64_ONLY));
        }
        Err(e) => return Err(e.into()),
    };
    let mut names = Vec::new();
    let mut lines = Vec::new();
    for sh in &shares {
        let name = sh.netname.as_ref().map(|n| n.to_string()).unwrap_or_default();
        lines.push(format!("{name} {:?}", sh.share_type.kind()));
        names.push(name);
    }
    let has = |want: &str| names.iter().any(|n| n.eq_ignore_ascii_case(want));
    check(has(&t.share), || format!("{} not in {names:?}", t.share))?;
    if let Some(p) = &t.print_share {
        check(has(p), || format!("{p} not in {names:?}"))?;
    }
    s.close().await?;
    Ok(lines)
}
