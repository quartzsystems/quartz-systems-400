// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `smbtest probe HOST PORT`: NEGOTIATE only, no sign-on, so it is safe
//! against any server (the oracles, Windows' own SMB server).

use smb::connection::EncryptionMode;
use smb::{Client, ClientConfig};

use super::negotiate::describe;
use crate::conn::{DIALECTS, dialect_config, dialect_name};
use crate::{Outcome, Report, Target};

/// Each dialect alone, with encryption allowed and disabled.
pub async fn run(t: &Target) -> Report {
    let mut r = Report::default();
    for d in DIALECTS {
        for (enc, tag) in [
            (EncryptionMode::Allowed, ""),
            (EncryptionMode::Disabled, "-noenc"),
        ] {
            let client = Client::new(ClientConfig {
                dfs: false,
                connection: dialect_config(t, d, enc),
                ..ClientConfig::default()
            });
            let outcome = match client.connect(&t.host).await {
                Ok(c) => match c.conn_info() {
                    Some(info) => Outcome::Ok(describe(info)),
                    None => Outcome::Fail("no negotiated state".into()),
                },
                Err(e) => Outcome::Fail(crate::Error::from(e).to_string()),
            };
            let _ = client.close().await;
            r.push(format!("probe-{}{tag}", dialect_name(d)), outcome);
        }
    }
    r
}
