// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server under test and connections to it: a sign-on, a tree, a
//! client from a `ClientGuid` of its own.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};

use smb_server::ntstatus::STATUS_SUCCESS;
use smb_server::wire::header::Command;
use smb_server::wire::messages::{FileId, SessionSetupResponse, TreeConnectResponse};
use tokio::net::TcpStream;

use crate::report::status;
use crate::smbc::auth::{Login, LoginOptions, login_with, request};
use crate::smbc::client::{
    Client, Create, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_GENERIC_READ, FILE_OPEN_IF, SHARE_ALL,
};
use crate::smbc::{durable, parse_response_header, tree_connect_body};

/// SMB 2.1.
pub const SMB21: u16 = 0x0210;
/// SMB 3.0.2.
pub const SMB302: u16 = 0x0302;
/// SMB 3.1.1.
pub const SMB311: u16 = 0x0311;

/// What the options name beyond the positional arguments.
#[derive(Default)]
pub struct Options {
    /// A second profile (`--user2`).
    pub user2: Option<String>,
    /// Its password (`SMBPROBE_PASSWORD2`).
    pub password2: Option<String>,
    /// A print share.
    pub print_share: Option<String>,
    /// A file share with a maximum of one user.
    pub max_share: Option<String>,
    /// A file share that requires encryption.
    pub enc_share: Option<String>,
    /// A user name with no profile, signed on as the guest.
    pub guest_user: Option<String>,
    /// The server's minimum dialect.
    pub min_dialect: Option<u16>,
    /// A file in `print$` and what it holds.
    pub driver_file: Option<(String, Vec<u8>)>,
    /// What the harness writes into the file `lease-break-ifs` holds.
    pub ifs_text: Option<String>,
}

/// The server under test.
pub struct Target {
    pub addr: SocketAddr,
    /// The server's name as the client calls it.
    pub host: String,
    pub user: String,
    pub password: String,
    /// The read/write file share.
    pub share: String,
    /// The directory on the share the scenarios work under.
    pub scratch: String,
    /// This run's name: `probe-PID-MILLIS`.
    pub run: String,
    pub opt: Options,
    /// This run's `ClientGuid`s start with these bytes.
    seed: [u8; 12],
    next: AtomicU8,
}

impl Target {
    pub fn new(
        addr: SocketAddr,
        host: &str,
        user: &str,
        password: &str,
        share: &str,
        scratch: &str,
        opt: Options,
    ) -> Self {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let pid = std::process::id();
        let mut seed = [0u8; 12];
        seed[..4].copy_from_slice(&pid.to_le_bytes());
        seed[4..12].copy_from_slice(&u64::try_from(millis).unwrap_or(0).to_le_bytes());
        Self {
            addr,
            host: host.to_owned(),
            user: user.to_owned(),
            password: password.to_owned(),
            share: share.to_owned(),
            scratch: scratch.trim_matches(['\\', '/']).replace('/', "\\"),
            run: format!("probe-{pid}-{millis}"),
            opt,
            seed,
            next: AtomicU8::new(1),
        }
    }

    /// A `ClientGuid` no other connection of this run sends: another
    /// machine, so on QS/400 another `QZLSFILE` job.
    pub fn machine(&self) -> [u8; 16] {
        let mut g = [0u8; 16];
        g[..12].copy_from_slice(&self.seed);
        g[12] = 0x5A;
        g[15] = self.next.fetch_add(1, Ordering::Relaxed);
        g
    }

    /// `\\HOST\SHARE`.
    pub fn unc(&self, share: &str) -> String {
        format!(r"\\{}\{share}", self.host)
    }

    /// This run's directory on the share, relative to it.
    pub fn dir(&self) -> String {
        format!(r"{}\{}", self.scratch, self.run)
    }

    /// `name` in this run's directory.
    pub fn file(&self, name: &str) -> String {
        format!(r"{}\{name}", self.dir())
    }

    /// The user and password of the second profile, or the first's.
    pub fn second(&self) -> (&str, &str) {
        match (&self.opt.user2, &self.opt.password2) {
            (Some(u), Some(p)) => (u, p),
            _ => (&self.user, &self.password),
        }
    }
}

/// A dialect's wire value from its name.
pub fn dialect(name: &str) -> Option<u16> {
    Some(match name {
        "2.0.2" => 0x0202,
        "2.1" => SMB21,
        "3.0" => 0x0300,
        "3.0.2" => SMB302,
        "3.1.1" => SMB311,
        _ => return None,
    })
}

/// A connection from `guid` at `dialect`, signed on as `user`.
pub async fn session(
    t: &Target,
    guid: [u8; 16],
    dialect: u16,
    user: &str,
    password: &str,
    previous: u64,
) -> Result<(TcpStream, Login), String> {
    let mut s = durable::negotiated(t.addr, dialect, guid).await;
    let opts = LoginOptions {
        previous_session_id: previous,
        ..LoginOptions::default()
    };
    let l = login_with(&mut s, 1, 0, user, password, opts).await;
    if l.status != STATUS_SUCCESS {
        return Err(format!("sign-on as {user}: {}", status(l.status)));
    }
    Ok((s, l))
}

/// The session flags of a successful sign-on.
pub fn session_flags(l: &Login) -> u16 {
    SessionSetupResponse::parse(&l.frame[64..]).map_or(0, |r| r.session_flags)
}

/// `TREE_CONNECT` to `share` (`MessageId` 3); the status, `TreeId` and share
/// type.
pub async fn tree(t: &Target, s: &mut TcpStream, session_id: u64, share: &str) -> (u32, u32, u8) {
    let f = request(
        s,
        Command::TreeConnect,
        3,
        session_id,
        0,
        &tree_connect_body(&t.unc(share)),
    )
    .await;
    let (h, body) = parse_response_header(&f);
    let st = h.channel_sequence_status;
    if st != STATUS_SUCCESS {
        return (st, 0, 0);
    }
    let kind = TreeConnectResponse::parse(body).map_or(0, |r| r.share_type);
    (st, h.tree_id().unwrap_or(0), kind)
}

/// A client from `guid` at `dialect`, signed on as `user`, on `share`.
pub async fn client_as(
    t: &Target,
    guid: [u8; 16],
    dialect: u16,
    (user, password): (&str, &str),
    share: &str,
) -> Result<Client, String> {
    let (mut s, l) = session(t, guid, dialect, user, password, 0).await?;
    let (st, tree_id, _) = tree(t, &mut s, l.session_id, share).await;
    if st != STATUS_SUCCESS {
        return Err(format!("tree connect {share} as {user}: {}", status(st)));
    }
    Ok(Client::from_parts(s, l.session_id, tree_id))
}

/// A client of the run's user on the file share from a machine of its own.
pub async fn client(t: &Target, dialect: u16) -> Result<Client, String> {
    client_as(t, t.machine(), dialect, (&t.user, &t.password), &t.share).await
}

/// This run's directory, made when missing.
pub async fn run_dir(t: &Target, c: &mut Client) -> Result<(), String> {
    for dir in [t.scratch.clone(), t.dir()] {
        let r = c
            .create(Create {
                name: &dir,
                access: FILE_GENERIC_READ,
                share: SHARE_ALL,
                disposition: FILE_OPEN_IF,
                options: FILE_DIRECTORY_FILE,
                ..Create::default()
            })
            .await
            .map_err(|e| format!("directory {dir}: {}", status(e)))?;
        c.close(r.file_id).await;
    }
    Ok(())
}

/// A new file `name` in the run's directory holding `data`, closed.
pub async fn new_file(t: &Target, c: &mut Client, name: &str, data: &[u8]) -> Result<(), String> {
    run_dir(t, c).await?;
    let path = t.file(name);
    let id = c
        .create(Create {
            name: &path,
            disposition: FILE_CREATE,
            ..Create::default()
        })
        .await
        .map_err(|e| format!("create {path}: {}", status(e)))?
        .file_id;
    write_all(c, id, data).await?;
    c.close(id).await;
    Ok(())
}

/// Writes `data` at 0.
pub async fn write_all(c: &mut Client, id: FileId, data: &[u8]) -> Result<(), String> {
    match c.write(id, 0, data).await {
        Ok(n) if n as usize == data.len() => Ok(()),
        Ok(n) => Err(format!("wrote {n} of {} bytes", data.len())),
        Err(e) => Err(format!("write: {}", status(e))),
    }
}

/// Fails with `what` unless `got` is `want`.
pub fn expect(what: &str, got: u32, want: u32) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "{what}: {} where {} was expected",
            status(got),
            status(want)
        ))
    }
}
