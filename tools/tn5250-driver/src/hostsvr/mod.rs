// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `hostsvr-trace`: run a client program through the recording proxy
//! and write one trace per server it talked to, as
//! `OUT/<server>/<scenario>.trace`.

pub mod proxy;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};

use crate::Target;
use crate::env::Env;
use proxy::{Proxy, ProxyConfig};

/// The ports recorded by default: the port mapper and the plain host
/// server ports.
pub const DEFAULT_PORTS: [u16; 8] = [449, 8470, 8471, 8472, 8473, 8474, 8475, 8476];

/// What a recording run needs.
#[derive(Clone, Debug)]
pub struct TraceRun {
    /// The oracle.
    pub target: Target,
    /// Where traces go.
    pub out: PathBuf,
    /// The scenario's name (the trace files' stem).
    pub scenario: String,
    /// The ports.
    pub ports: Vec<u16>,
    /// How long open connections get to close after the client exits.
    pub settle: Duration,
    /// The client command; it reads `QS_HS_HOST`, `QS_HS_USER` and
    /// `QS_HS_PASS` from its environment.
    pub command: Vec<String>,
}

/// Run the client through the proxy and write the traces.
pub fn run(r: &TraceRun) -> Result<Vec<PathBuf>> {
    if r.command.is_empty() {
        bail!("no client command given");
    }
    let e = Env::load()?;
    let creds = e.credentials(r.target, None)?;
    let cfg = ProxyConfig {
        upstream: creds.host.clone(),
        ports: r.ports.clone(),
        forbidden: proxy::forbidden_forms(&creds.password),
        user: creds.user.clone(),
        password: creds.password.clone(),
    };
    let p = Proxy::start(&cfg)?;
    let status = Command::new(&r.command[0])
        .args(&r.command[1..])
        .env("QS_HS_HOST", "127.0.0.1")
        .env("QS_HS_USER", &creds.user)
        .env("QS_HS_PASS", &creds.password)
        .status()
        .with_context(|| format!("run {}", r.command[0]))?;
    println!("client exited: {status}");
    let traces = p.finish(r.settle, &r.scenario)?;
    let mut written = Vec::new();
    for t in traces {
        let path = r.out.join(&t.server).join(format!("{}.trace", r.scenario));
        write(&path, &t.write())?;
        let chunks: usize = t.connections.iter().map(|c| c.chunks().count()).sum();
        println!(
            "{}: {} connections, {chunks} chunks",
            path.display(),
            t.connections.len()
        );
        written.push(path);
    }
    Ok(written)
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))
}
