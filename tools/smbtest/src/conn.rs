// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Clients, sessions and opens against the target.
//!
//! The client crate keeps one connection per server address in each
//! [`Client`], so every independent connection is its own `Client`. They
//! all send one `ClientGuid`, as Windows' redirector does for a machine.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use smb::connection::EncryptionMode;
use smb::{
    Client, ClientConfig, ConnectionConfig, CreateOptions, Dialect, FileAccessMask, FileAttributes,
    FileCreateArgs, Resource, UncPath,
};
use smb::connection::connection_info::ConnectionInfo;

use crate::{Error, Target};

/// This run's `ClientGuid`, one for every connection.
fn client_guid() -> smb::Guid {
    static GUID: OnceLock<smb::Guid> = OnceLock::new();
    *GUID.get_or_init(smb::Guid::generate)
}

/// How long any one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The connection settings every scenario starts from: the target's port,
/// NTLM only, no DFS, no multichannel, SMB2 negotiate.
pub fn base_config(t: &Target) -> ConnectionConfig {
    let mut c = ConnectionConfig {
        port: Some(t.port),
        timeout: Some(TIMEOUT),
        smb2_only_negotiate: true,
        client_name: Some("SMBTEST".into()),
        ..ConnectionConfig::default()
    };
    c.auth_methods.ntlm = true;
    c.auth_methods.kerberos = false;
    c
}

/// A connection limited to one dialect, encryption as given.
pub fn dialect_config(t: &Target, d: Dialect, enc: EncryptionMode) -> ConnectionConfig {
    ConnectionConfig { min_dialect: Some(d), max_dialect: Some(d), encryption_mode: enc, ..base_config(t) }
}

/// A connected share.
pub struct Share {
    /// The client holding the connection, session and tree.
    pub client: Client,
    /// `\\host\share`.
    pub unc: UncPath,
}

impl Share {
    /// Connects and signs on to the target's file share.
    pub async fn connect(t: &Target, config: ConnectionConfig) -> Result<Share, Error> {
        Share::connect_as(t, config, &t.user, &t.password, &t.share).await
    }

    /// Connects to `share` as `user`.
    pub async fn connect_as(
        t: &Target,
        config: ConnectionConfig,
        user: &str,
        password: &str,
        share: &str,
    ) -> Result<Share, Error> {
        let client = Client::new(ClientConfig {
            dfs: false,
            connection: config,
            client_guid: client_guid(),
            ..ClientConfig::default()
        });
        let unc = UncPath::new(&t.host)?.with_share(share)?;
        client.share_connect(&unc, user, password.to_string()).await?;
        Ok(Share { client, unc })
    }

    /// What was negotiated.
    pub async fn info(&self) -> Result<Arc<ConnectionInfo>, Error> {
        let conn = self.client.get_connection(self.unc.server()).await?;
        conn.conn_info().cloned().ok_or(Error::Check("connection has no negotiated state".into()))
    }

    /// `\\host\share\path`.
    pub fn path(&self, path: &str) -> UncPath {
        self.unc.clone().with_path(path)
    }

    /// Opens or creates `path` with `args`.
    pub async fn open(&self, path: &str, args: &FileCreateArgs) -> Result<Resource, Error> {
        Ok(self.client.create_file(&self.path(path), args).await?)
    }

    /// Creates a new file, failing if it exists.
    pub async fn create_file(&self, path: &str) -> Result<smb::File, Error> {
        let args = FileCreateArgs::make_create_new(FileAttributes::new().with_archive(true), CreateOptions::new());
        file(self.open(path, &args).await?)
    }

    /// Opens an existing file for reading and writing.
    pub async fn open_file(&self, path: &str, write: bool) -> Result<smb::File, Error> {
        let access = FileAccessMask::new()
            .with_generic_read(true)
            .with_generic_write(write)
            .with_delete(true);
        file(self.open(path, &FileCreateArgs::make_open_existing(access)).await?)
    }

    /// Creates a new directory.
    pub async fn create_dir(&self, path: &str) -> Result<(), Error> {
        let args = FileCreateArgs::make_create_new(
            FileAttributes::new().with_directory(true),
            CreateOptions::new().with_directory_file(true),
        );
        self.open(path, &args).await?.as_dir().ok_or(Error::Check(format!("{path} is not a directory")))?.close().await?;
        Ok(())
    }

    /// Opens a directory to list or watch.
    pub async fn open_dir(&self, path: &str) -> Result<smb::Directory, Error> {
        let access = FileAccessMask::new().with_generic_read(true).with_delete(true);
        match self.open(path, &FileCreateArgs::make_open_existing(access)).await? {
            Resource::Directory(d) => Ok(d),
            _ => Err(Error::Check(format!("{path} opened, but not as a directory"))),
        }
    }

    /// Ends the session and the connection.
    pub async fn close(self) -> Result<(), Error> {
        Ok(self.client.close().await?)
    }
}

fn file(r: Resource) -> Result<smb::File, Error> {
    match r {
        Resource::File(f) => Ok(f),
        _ => Err(Error::Check("opened, but not as a file".into())),
    }
}

/// `3.1.1` and so on.
pub fn dialect_name(d: Dialect) -> &'static str {
    match d {
        Dialect::Smb0202 => "2.0.2",
        Dialect::Smb021 => "2.1",
        Dialect::Smb030 => "3.0",
        Dialect::Smb0302 => "3.0.2",
        Dialect::Smb0311 => "3.1.1",
    }
}

/// Every dialect the scenarios negotiate, oldest first.
pub const DIALECTS: [Dialect; 5] =
    [Dialect::Smb0202, Dialect::Smb021, Dialect::Smb030, Dialect::Smb0302, Dialect::Smb0311];
