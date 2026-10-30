// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The relay over a Unix socket: one connection per request, the
//! descriptor in `SCM_RIGHTS` beside the message bytes.

use std::io::{IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer, SendAncillaryMessage, SendFlags,
};

use super::{State, Taken, op, status};
use crate::Errno;
use crate::pass::JobId;
use crate::sys::Fd;

fn io(e: &std::io::Error) -> Errno {
    ifs::errno::linux::from_linux(e.raw_os_error().unwrap_or(0))
}

fn rx(e: rustix::io::Errno) -> Errno {
    ifs::errno::linux::from_linux(e.raw_os_error())
}

/// Send `data`, with `fd` when given.
fn send(sock: BorrowedFd<'_>, data: &[u8], fd: Option<BorrowedFd<'_>>) -> Result<(), Errno> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds;
    if let Some(f) = fd {
        fds = [f];
        control.push(SendAncillaryMessage::ScmRights(&fds));
    }
    rustix::net::sendmsg(sock, &[IoSlice::new(data)], &mut control, SendFlags::NOSIGNAL).map_err(rx)?;
    Ok(())
}

/// Receive up to `buf.len()` bytes and a descriptor, if one came.
fn recv(sock: BorrowedFd<'_>, buf: &mut [u8]) -> Result<(usize, Option<OwnedFd>), Errno> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let r = rustix::net::recvmsg(sock, &mut [IoSliceMut::new(buf)], &mut control, RecvFlags::CMSG_CLOEXEC)
        .map_err(rx)?;
    let mut fd = None;
    for m in control.drain() {
        if let RecvAncillaryMessage::ScmRights(mut fds) = m
            && let Some(f) = fds.next()
        {
            fd = Some(f);
        }
    }
    Ok((r.bytes, fd))
}

fn id(b: &[u8]) -> JobId {
    let mut a = [0u8; 16];
    let n = b.len().min(16);
    a[..n].copy_from_slice(&b[..n]);
    a
}

/// Give `fd` to `target`, from `source`, with a note (at most 255 bytes).
pub fn give(path: &str, fd: &Fd, target: &JobId, source: &JobId, note: &[u8]) -> Result<(), Errno> {
    let s = UnixStream::connect(path).map_err(|e| io(&e))?;
    let note = &note[..note.len().min(255)];
    let mut m = vec![op::GIVE];
    m.extend_from_slice(target);
    m.extend_from_slice(source);
    m.push(u8::try_from(note.len()).unwrap_or(255));
    m.extend_from_slice(note);
    send(s.as_fd(), &m, Some(fd.0.as_fd()))?;
    let mut b = [0u8; 1];
    let (n, _) = recv(s.as_fd(), &mut b)?;
    if n == 1 && b[0] == status::OK { Ok(()) } else { Err(Errno::EINVAL) }
}

/// Take a descriptor given to `me` (from `source` when given), waiting
/// up to `wait_ms` (0 forever); `None` when none came in time.
pub fn take(path: &str, me: &JobId, source: Option<&JobId>, wait_ms: u32) -> Result<Option<Taken>, Errno> {
    let s = UnixStream::connect(path).map_err(|e| io(&e))?;
    let mut m = vec![op::TAKE];
    m.extend_from_slice(me);
    m.extend_from_slice(source.unwrap_or(&[0; 16]));
    m.extend_from_slice(&wait_ms.to_be_bytes());
    send(s.as_fd(), &m, None)?;
    let mut b = [0u8; 300];
    let (n, fd) = recv(s.as_fd(), &mut b)?;
    match (b.first().copied(), fd) {
        (Some(status::OK), Some(fd)) if n >= 18 => {
            let len = usize::from(b[17]);
            Ok(Some(Taken {
                fd: Fd(fd),
                source: id(&b[1..17]),
                note: b[18..(18 + len).min(n)].to_vec(),
            }))
        }
        (Some(status::TIMEOUT), _) => Ok(None),
        _ => Err(Errno::EINVAL),
    }
}

/// How many takers wait as `pool`.
pub fn idle(path: &str, pool: &JobId) -> Result<u32, Errno> {
    let s = UnixStream::connect(path).map_err(|e| io(&e))?;
    let mut m = vec![op::IDLE];
    m.extend_from_slice(pool);
    send(s.as_fd(), &m, None)?;
    let mut b = [0u8; 4];
    let (n, _) = recv(s.as_fd(), &mut b)?;
    if n == 4 { Ok(u32::from_be_bytes(b)) } else { Err(Errno::EINVAL) }
}

/// One connection's request.
fn answer(s: &UnixStream, st: &State<OwnedFd>) -> Result<(), Errno> {
    let mut b = [0u8; 300];
    let (n, fd) = recv(s.as_fd(), &mut b)?;
    match b.first().copied() {
        Some(op::GIVE) if n >= 34 => {
            let Some(fd) = fd else {
                return send(s.as_fd(), &[status::BAD], None);
            };
            let len = usize::from(b[33]);
            st.give(id(&b[1..17]), id(&b[17..33]), b[34..(34 + len).min(n)].to_vec(), fd);
            send(s.as_fd(), &[status::OK], None)
        }
        Some(op::TAKE) if n >= 37 => {
            let src = id(&b[17..33]);
            let source = (src != [0; 16]).then_some(src);
            let ms = u32::from_be_bytes([b[33], b[34], b[35], b[36]]);
            let wait = (ms > 0).then(|| Duration::from_millis(u64::from(ms)));
            match st.take(id(&b[1..17]), source, wait) {
                Some((from, note, fd)) => {
                    let mut m = vec![status::OK];
                    m.extend_from_slice(&from);
                    m.push(u8::try_from(note.len()).unwrap_or(255));
                    m.extend_from_slice(&note);
                    // A taker that went away leaves the descriptor to close.
                    send(s.as_fd(), &m, Some(fd.as_fd()))
                }
                None => send(s.as_fd(), &[status::TIMEOUT], None),
            }
        }
        Some(op::IDLE) if n >= 17 => send(s.as_fd(), &st.idle(&id(&b[1..17])).to_be_bytes(), None),
        _ => send(s.as_fd(), &[status::BAD], None),
    }
}

/// Serve the relay on `path` (a thread per connection). Returns when
/// the socket cannot be made.
pub fn serve(path: &str) -> std::io::Result<()> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(p);
    let l = UnixListener::bind(p)?;
    let st: Arc<State<OwnedFd>> = Arc::default();
    for c in l.incoming() {
        let Ok(c) = c else { continue };
        let st = st.clone();
        std::thread::spawn(move || {
            if let Err(e) = answer(&c, &st) {
                tracing::debug!("descriptor relay: {e:?}");
            }
        });
    }
    Ok(())
}
