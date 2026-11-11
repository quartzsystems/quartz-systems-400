// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who may connect: the guest profile, a share's maximum users, a share
//! that requires encryption, and the server's minimum dialect.

use std::sync::Arc;
use std::time::{Duration, Instant};

use smb_server::ntstatus::{
    STATUS_ACCESS_DENIED, STATUS_NOT_SUPPORTED, STATUS_REQUEST_NOT_ACCEPTED, STATUS_SUCCESS,
};
use smb_server::wire::header::Command;
use smb_server::wire::messages::{
    CloseRequest, CreateResponse, NegotiateResponse, ReadRequest, ReadResponse,
    SessionSetupResponse,
};
use tokio::net::TcpStream;

use super::Done;
use crate::conn::{self, SMB21, SMB302, Target, expect};
use crate::crypto::{Cipher, EncryptedSession};
use crate::report::status;
use crate::smbc::auth::{LoginOptions, login_with, request, status as frame_status};
use crate::smbc::client::{Create, DELETE, FILE_CREATE, FILE_DELETE_ON_CLOSE, RW, write_request};
use crate::smbc::{negotiate_body_from, parse_response_header};

/// A user name with no profile signs on as the guest: a session flagged
/// `IS_GUEST` (no key, so unsigned), which reaches `IPC$`.
pub async fn guest_logon(t: Arc<Target>) -> Done {
    let user = t.opt.guest_user.clone().unwrap_or_default();
    let (mut s, l) = conn::session(&t, t.machine(), SMB302, &user, "not-a-password", 0).await?;
    let flags = conn::session_flags(&l);
    if flags & SessionSetupResponse::FLAG_IS_GUEST == 0 {
        return Err(format!("session flags 0x{flags:04X}: not a guest session"));
    }
    let (st, _, _) = conn::tree(&t, &mut s, l.session_id, "IPC$").await;
    expect("the guest's tree connect to IPC$", st, STATUS_SUCCESS)?;
    Ok(vec![format!(
        "{user}: session flags 0x{flags:04X} (guest), IPC$ connected"
    )])
}

/// A share with a maximum of one user: one machine's user connects,
/// another machine's (the second profile) is refused while the first is
/// connected, and connects once it has gone.
pub async fn max_users(t: Arc<Target>) -> Done {
    let share = t.opt.max_share.clone().unwrap_or_default();
    let (mut s1, l1) = conn::session(&t, t.machine(), SMB302, &t.user, &t.password, 0).await?;
    let (st, _, _) = conn::tree(&t, &mut s1, l1.session_id, &share).await;
    expect("the first user's tree connect", st, STATUS_SUCCESS)?;
    let (user2, password2) = t.second();
    let (mut s2, l2) = conn::session(&t, t.machine(), SMB302, user2, password2, 0).await?;
    let (st, _, _) = conn::tree(&t, &mut s2, l2.session_id, &share).await;
    expect(
        "a second user's tree connect",
        st,
        STATUS_REQUEST_NOT_ACCEPTED,
    )?;
    drop(s1);
    // The session table hears the first one's end within a few seconds.
    let end = Instant::now() + Duration::from_secs(60);
    let mut tries = 0;
    loop {
        tries += 1;
        let (mut s, l) = conn::session(&t, t.machine(), SMB302, user2, password2, 0).await?;
        let (st, _, _) = conn::tree(&t, &mut s, l.session_id, &share).await;
        if st == STATUS_SUCCESS {
            break;
        }
        if Instant::now() > end {
            return Err(format!("after the first user left: {}", status(st)));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(vec![
        format!(
            "{} connected; {user2} refused STATUS_REQUEST_NOT_ACCEPTED",
            t.user
        ),
        format!("{user2} connected once the first left ({tries} tries)"),
    ])
}

/// A share that requires encryption: an unencrypted 2.1 or 3.0.2 session's
/// tree connect is refused; a sealed 3.1.1 session connects and a file
/// round trips through it.
pub async fn encrypt_share(t: Arc<Target>) -> Done {
    let share = t.opt.enc_share.clone().unwrap_or_default();
    let mut lines = Vec::new();
    for d in [SMB21, SMB302] {
        if t.opt.min_dialect.is_some_and(|m| d < m) {
            continue;
        }
        let (mut s, l) = conn::session(&t, t.machine(), d, &t.user, &t.password, 0).await?;
        let (st, _, _) = conn::tree(&t, &mut s, l.session_id, &share).await;
        expect(
            &format!("an unencrypted tree connect at 0x{d:04X}"),
            st,
            STATUS_ACCESS_DENIED,
        )?;
        lines.push(format!("unencrypted at 0x{d:04X}: STATUS_ACCESS_DENIED"));
    }
    let mut e =
        EncryptedSession::smb311_from(t.addr, &Cipher::ALL, &t.user, &t.password, t.machine())
            .await;
    let tc = e.tree_connect(&t.unc(&share)).await;
    expect("the sealed tree connect", frame_status(&tc), STATUS_SUCCESS)?;
    let name = format!("{}.enc.txt", t.run);
    let body = Create {
        name: &name,
        access: RW | DELETE,
        disposition: FILE_CREATE,
        options: FILE_DELETE_ON_CLOSE,
        ..Create::default()
    }
    .body();
    let f = e.send_encrypted(Command::Create, &body).await;
    expect("sealed CREATE", frame_status(&f), STATUS_SUCCESS)?;
    let id = CreateResponse::parse(&f[64..])
        .map_err(|e| format!("CREATE response: {e}"))?
        .file_id;
    let data = b"sealed round trip";
    let mut w = Vec::new();
    write_request(id, 0, data)
        .write_to(&mut w)
        .map_err(|e| format!("WRITE: {e}"))?;
    let f = e.send_encrypted(Command::Write, &w).await;
    expect("sealed WRITE", frame_status(&f), STATUS_SUCCESS)?;
    let mut r = Vec::new();
    ReadRequest {
        structure_size: 49,
        padding: ReadResponse::STANDARD_DATA_OFFSET,
        flags: 0,
        length: 64,
        offset: 0,
        file_id: id,
        minimum_count: 0,
        channel: 0,
        remaining_bytes: 0,
        read_channel_info_offset: 0,
        read_channel_info_length: 0,
        buffer: vec![0],
    }
    .write_to(&mut r)
    .map_err(|e| format!("READ: {e}"))?;
    let f = e.send_encrypted(Command::Read, &r).await;
    expect("sealed READ", frame_status(&f), STATUS_SUCCESS)?;
    let got = ReadResponse::parse(&f[64..])
        .map_err(|e| format!("READ response: {e}"))?
        .data;
    if got != data {
        return Err(format!("sealed READ gave {got:?}"));
    }
    let mut c = Vec::new();
    CloseRequest {
        structure_size: 24,
        flags: 0,
        reserved: 0,
        file_id: id,
    }
    .write_to(&mut c)
    .map_err(|e| format!("CLOSE: {e}"))?;
    let _ = e.send_encrypted(Command::Close, &c).await;
    lines.push(format!(
        "sealed 3.1.1 ({:?}): connected, a file round trips",
        e.cipher
    ));
    Ok(lines)
}

/// With the server's minimum dialect set: a NEGOTIATE offering only
/// lower dialects is refused, one offering the minimum too gets it or
/// higher, and a sign-on there works.
pub async fn min_dialect(t: Arc<Target>) -> Done {
    let min = t.opt.min_dialect.unwrap_or(0x0202);
    let lower: Vec<u16> = [0x0202, SMB21, 0x0300, SMB302]
        .into_iter()
        .filter(|d| *d < min)
        .collect();
    if lower.is_empty() {
        return Err("no dialect below the minimum to offer".into());
    }
    let mut s = TcpStream::connect(t.addr)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let body = negotiate_body_from(&lower, 1, 0, t.machine());
    let f = request(&mut s, Command::Negotiate, 0, 0, 0, &body).await;
    let refused = parse_response_header(&f).0.channel_sequence_status;
    expect("NEGOTIATE below the minimum", refused, STATUS_NOT_SUPPORTED)?;
    let at = if min == 0x0311 {
        SMB302
    } else {
        min.max(SMB302)
    };
    if min == 0x0311 {
        let e =
            EncryptedSession::smb311_from(t.addr, &Cipher::ALL, &t.user, &t.password, t.machine())
                .await;
        return Ok(vec![
            format!("offering {lower:04X?}: STATUS_NOT_SUPPORTED"),
            format!("3.1.1 signed on (dialect 0x{:04X})", e.dialect),
        ]);
    }
    let mut offered = lower.clone();
    offered.push(at);
    let mut s = TcpStream::connect(t.addr)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let body = negotiate_body_from(&offered, 1, 0, t.machine());
    let f = request(&mut s, Command::Negotiate, 0, 0, 0, &body).await;
    expect(
        "NEGOTIATE with the minimum",
        frame_status(&f),
        STATUS_SUCCESS,
    )?;
    let chosen = NegotiateResponse::parse(&f[64..])
        .map_err(|e| format!("NEGOTIATE response: {e}"))?
        .dialect_revision;
    if chosen < min {
        return Err(format!(
            "chose 0x{chosen:04X} below the minimum 0x{min:04X}"
        ));
    }
    let l = login_with(&mut s, 1, 0, &t.user, &t.password, LoginOptions::default()).await;
    expect("sign-on", l.status, STATUS_SUCCESS)?;
    Ok(vec![
        format!("offering {lower:04X?}: STATUS_NOT_SUPPORTED"),
        format!("offering {offered:04X?}: 0x{chosen:04X}, signed on"),
    ])
}
