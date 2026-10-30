// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server: configuration, host keys and the services it calls, and
//! [`Server::serve`] for one connection over any byte stream.

use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;

use sockets::PortRules;

use crate::auth::{Accounts, Peer};
use crate::config::{Config, paths};
use crate::conn::{self, Forwarder, TcpForwarder};
use crate::error::SshResult;
use crate::io::{FileSystem, files};
use crate::keys::{HostKeyError, HostKeys};
use crate::session::SessionHost;

/// What the server calls out to.
#[derive(Clone)]
pub struct Deps {
    /// Profiles and the password check.
    pub accounts: Arc<dyn Accounts>,
    /// Runs sessions as jobs.
    pub host: Arc<dyn SessionHost>,
    /// The IFS.
    pub fs: Arc<dyn FileSystem>,
    /// The `ADDTCPPORT` restrictions, for remote forwards.
    pub ports: Arc<dyn PortRules + Send + Sync>,
    /// Makes forwarded connections.
    pub forwarder: Arc<dyn Forwarder>,
}

impl Deps {
    /// The services, with no port restrictions and forwarding over the
    /// kernel's TCP.
    pub fn new(
        accounts: Arc<dyn Accounts>,
        host: Arc<dyn SessionHost>,
        fs: Arc<dyn FileSystem>,
    ) -> Self {
        Self {
            accounts,
            host,
            fs,
            ports: Arc::new(sockets::AllowAll),
            forwarder: Arc::new(TcpForwarder),
        }
    }
}

/// The server could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The configuration file could not be read or written.
    #[error("{path}: {errno}")]
    Config {
        /// The file.
        path: String,
        /// Why.
        errno: ifs::Errno,
    },
    /// A host key.
    #[error(transparent)]
    HostKey(#[from] HostKeyError),
    /// No host key is configured.
    #[error("no host key is available")]
    NoHostKey,
}

/// An SSH server.
pub struct Server {
    /// The configuration.
    pub config: Arc<Config>,
    /// The host keys.
    pub keys: Arc<HostKeys>,
    /// The services.
    pub deps: Deps,
}

impl Server {
    /// A server from parts already loaded.
    #[must_use]
    pub fn new(config: Config, keys: HostKeys, deps: Deps) -> Self {
        Self {
            config: Arc::new(config),
            keys: Arc::new(keys),
            deps,
        }
    }

    /// Start as `STRTCPSVR SERVER(*SSHD)` does: read `config_path`
    /// (writing the default configuration there first if there is none),
    /// then load the host keys, making those missing.
    pub fn start(config_path: &str, deps: Deps) -> Result<Self, StartError> {
        let ifs = deps.fs.system();
        let err = |errno| StartError::Config {
            path: config_path.to_string(),
            errno,
        };
        if !files::exists(&*ifs, config_path) {
            if let Some((dir, _)) = config_path.rsplit_once('/')
                && !dir.is_empty()
            {
                files::mkdir_p(&*ifs, dir, 0o755).map_err(err)?;
            }
            files::write(&*ifs, config_path, paths::default_config().as_bytes(), 0o644)
                .map_err(err)?;
            tracing::info!(path = config_path, "default sshd configuration written");
        }
        let config = Config::parse(&files::read_text(&*ifs, config_path).map_err(err)?);
        for n in &config.notes {
            tracing::info!(path = config_path, "{n}");
        }
        let comment = format!("QSYS@{}", release::RELEASE.full());
        let keys = HostKeys::load_or_generate(&*ifs, &config.host_key_paths(), &comment)?;
        if keys.0.is_empty() {
            return Err(StartError::NoHostKey);
        }
        drop(ifs);
        Ok(Self::new(config, keys, deps))
    }

    /// Serve one connection: `read` and `write` are the two halves of
    /// the client's byte stream. Returns when the connection ends.
    pub fn serve<R: Read + Send + 'static, W: Write>(
        &self,
        read: R,
        write: W,
        peer: Peer,
        local: Peer,
    ) -> SshResult<()> {
        conn::run(self, BufReader::new(read), write, peer, local)
    }

    /// Serve one accepted TCP connection.
    pub fn serve_tcp(&self, stream: &TcpStream) -> SshResult<()> {
        let addr = |a: std::io::Result<std::net::SocketAddr>| {
            a.map(|a| Peer {
                addr: a.ip().to_string(),
                port: a.port(),
            })
            .unwrap_or_default()
        };
        let peer = addr(stream.peer_addr());
        let local = addr(stream.local_addr());
        let _ = stream.set_nodelay(true);
        let read = stream.try_clone()?;
        let r = self.serve(read, stream, peer, local);
        let _ = stream.shutdown(Shutdown::Both);
        r
    }
}
