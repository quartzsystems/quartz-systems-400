// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qs400-updated`: the update service installed systems order PTFs
//! from (`SNDPTFORD`, `QSCHKUPD`). It mirrors the directory the PTF
//! workflow publishes into `--dir` every `--every` seconds and serves it
//! with the reference server (`docs/UPDATE_PROTOCOL.md`).
//!
//! ```text
//! qs400-updated [--listen 0.0.0.0:8080] [--dir /srv/qs400] [--prefix /qs400]
//!               [--source URL] [--trust HEX]... [--every 900]
//!               [--tls-cert chain.pem --tls-key key.pem] [--no-sync]
//! ```

mod serve;
mod sync;

use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;

/// Where the PTF workflow publishes the update service's files.
const SOURCE: &str =
    "https://github.com/quartzsystems/quartz-systems-400/releases/download/ptf-catalog/";

#[derive(Parser)]
#[command(about = "Serve the QS/400 update service, mirrored from its published files")]
struct Args {
    /// Address and port to listen on.
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,
    /// The service's directory (catalog.toml, ptf/, group/, problems/).
    #[arg(long, default_value = "/srv/qs400")]
    dir: PathBuf,
    /// The URL path the service is under (systems use .../qs400/v1/...).
    #[arg(long, default_value = "/qs400")]
    prefix: String,
    /// The base URL of the published files.
    #[arg(long, default_value = SOURCE)]
    source: String,
    /// A public key (64 hex digits) the catalog must be signed with;
    /// repeat for several. Without one the catalog is taken unchecked
    /// (installed systems still check it).
    #[arg(long)]
    trust: Vec<String>,
    /// Seconds between mirror passes.
    #[arg(long, default_value_t = 900)]
    every: u64,
    /// Serve the directory as it is, without mirroring.
    #[arg(long)]
    no_sync: bool,
    /// PEM certificate chain, leaf first, to terminate TLS here.
    #[arg(long, requires = "tls_key")]
    tls_cert: Option<PathBuf>,
    /// PEM PKCS#8 private key of the certificate.
    #[arg(long, requires = "tls_cert")]
    tls_key: Option<PathBuf>,
    /// Connections served at once.
    #[arg(long, default_value_t = 64)]
    limit: usize,
}

fn key(text: &str) -> Result<[u8; 32]> {
    let b = hex::decode(text.trim()).with_context(|| format!("--trust {text}"))?;
    let Ok(k) = <[u8; 32]>::try_from(b.as_slice()) else {
        bail!("--trust {text}: 32 bytes are needed");
    };
    Ok(k)
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let trusted = args
        .trust
        .iter()
        .map(|t| key(t))
        .collect::<Result<Vec<_>>>()?;
    let mut base = args.source.clone();
    if !base.ends_with('/') {
        base.push('/');
    }
    let src = sync::Source { base, trusted };
    std::fs::create_dir_all(&args.dir).with_context(|| args.dir.display().to_string())?;
    if !args.no_sync {
        let dir = args.dir.clone();
        let every = Duration::from_secs(args.every.max(60));
        std::thread::spawn(move || {
            loop {
                match sync::once(&src, &dir) {
                    Ok(p) => tracing::info!(
                        sequence = p.sequence,
                        fetched = p.fetched,
                        changed = p.changed,
                        "mirrored"
                    ),
                    Err(e) => tracing::warn!("mirror: {e:#}"),
                }
                std::thread::sleep(every);
            }
        });
    }
    let tls = match (&args.tls_cert, &args.tls_key) {
        (Some(c), Some(k)) => Some(serve::tls_config(c, k)?),
        _ => None,
    };
    let listener = TcpListener::bind(&args.listen).with_context(|| args.listen.clone())?;
    tracing::info!(
        listen = %args.listen,
        tls = tls.is_some(),
        prefix = %args.prefix,
        dir = %args.dir.display(),
        "serving"
    );
    let prefix = args.prefix.trim_end_matches('/').to_string();
    serve::run(
        &args.dir,
        &listener,
        serve::Listen {
            prefix,
            tls,
            limit: args.limit.max(1),
        },
    );
    Ok(())
}
