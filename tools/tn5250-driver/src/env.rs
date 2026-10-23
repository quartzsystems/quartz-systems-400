// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The oracle credentials: `QS_ORACLE_*` and `QS_PUB400_*` from the
//! process environment, else from `tools/conformance/oracle/.env`
//! found by walking up from the working directory. Nothing here is
//! ever written anywhere else.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::Target;

/// What a connection needs.
#[derive(Clone, Debug)]
pub struct Credentials {
    /// The host.
    pub host: String,
    /// The telnet port.
    pub port: u16,
    /// The user.
    pub user: String,
    /// The password.
    pub password: String,
}

/// The loaded variables.
#[derive(Debug, Default)]
pub struct Env {
    vars: HashMap<String, String>,
}

/// The file's location relative to the repository root.
pub const ENV_FILE: &str = "tools/conformance/oracle/.env";

impl Env {
    /// Load from the process environment, then the file.
    pub fn load() -> Result<Self> {
        let mut vars: HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("QS_"))
            .collect();
        if let Some(path) = find_file() {
            let text = std::fs::read_to_string(&path)?;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    vars.entry(k.trim().to_string())
                        .or_insert_with(|| v.trim().to_string());
                }
            }
        }
        Ok(Self { vars })
    }

    fn get(&self, key: &str) -> Result<String> {
        match self.vars.get(key) {
            Some(v) if !v.is_empty() => Ok(v.clone()),
            _ => bail!("{key} is not set (put it in {ENV_FILE} or the environment)"),
        }
    }

    /// The credentials of a target; `host` overrides the host and port.
    pub fn credentials(&self, target: Target, host: Option<&str>) -> Result<Credentials> {
        let (h, user, password) = match target {
            Target::Qs400 => (
                self.vars
                    .get("QS_QS400_HOST")
                    .cloned()
                    .unwrap_or_else(|| "127.0.0.1:2323".into()),
                self.vars
                    .get("QS_QS400_USER")
                    .cloned()
                    .unwrap_or_else(|| "QSECOFR".into()),
                self.vars
                    .get("QS_QS400_PASS")
                    .cloned()
                    .unwrap_or_else(|| "QSECOFR".into()),
            ),
            Target::Oracle => (
                self.get("QS_ORACLE_HOST")?,
                self.get("QS_ORACLE_USER")?,
                self.get("QS_ORACLE_PASS")?,
            ),
            Target::Pub400 => (
                self.get("QS_PUB400_HOST")?,
                self.get("QS_PUB400_USER")?,
                self.get("QS_PUB400_PASS")?,
            ),
        };
        let h = host.map_or(h, str::to_string);
        let (host, port) = match h.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => {
                (host.to_string(), port.parse::<u16>()?)
            }
            _ => (h, 23),
        };
        Ok(Credentials {
            host,
            port,
            user,
            password,
        })
    }
}

/// The `.env` file, looked for upward from the working directory.
pub fn find_file() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("QS_ORACLE_ENV") {
        return Some(PathBuf::from(p));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(ENV_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
