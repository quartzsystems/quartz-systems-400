// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSOSRV1`: the socket calls. The plain names read and write BSD 4.3
//! address structures; the `qso_*98` names (what `_XOPEN_SOURCE`
//! programs bind to) BSD 4.4 ones.

use mi::Job;
use mi::fd::Kind;
use sockets::layout::misc::{PollFd, iovec, msghdr};
use sockets::{AddrForm, Errno, SockAddr};

use super::abi::{
    bound_c, get_int, int, is_null, mem, opt_mem, ptr_at, put_int, ret,
};
use super::net;

type R<T> = Result<T, Errno>;

/// The kernel number behind a job descriptor.
pub(crate) fn kernel(j: &Job, sd: i32) -> R<i32> {
    j.fds.socket(sd).map_err(super::abi::errno)
}

/// A new job descriptor for a kernel socket; the socket closes when
/// the table is full.
pub(crate) fn adopt(j: &mut Job, k: i32) -> R<i32> {
    match j.fds.open(Kind::Socket(k)) {
        Ok(n) => Ok(n),
        Err(e) => {
            let _ = net(j).borrow().sockets.borrow_mut().close(k);
            Err(super::abi::errno(e))
        }
    }
}

fn addr_in(j: &Job, p: u128, len: i64, form: AddrForm) -> R<SockAddr> {
    let len = usize::try_from(int(len)).map_err(|_| Errno::EINVAL)?;
    if len < 2 {
        return Err(Errno::EINVAL);
    }
    SockAddr::decode(mem(j, p, len.min(128))?, form)
}

/// Store an address and its length when the caller asked for it.
pub(crate) fn addr_out(j: &Job, p: u128, lenp: u128, a: Option<&SockAddr>, form: AddrForm) -> R<()> {
    if is_null(p) || is_null(lenp) {
        return Ok(());
    }
    let avail = usize::try_from(get_int(j, lenp)?).map_err(|_| Errno::EINVAL)?;
    let n = match a {
        Some(a) => a.encode(form, mem(j, p, avail.min(a.size()))?),
        None => 0,
    };
    put_int(j, lenp, i32::try_from(n).unwrap_or(0))
}

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn len_of(v: i64) -> R<usize> {
    usize::try_from(int(v)).map_err(|_| Errno::EINVAL)
}

fn socket(j: &mut Job, domain: i64, ty: i64, proto: i64) -> i64 {
    let r = (|| {
        let k = net(j).borrow().sockets.borrow_mut().socket(int(domain), int(ty), int(proto))?;
        adopt(j, k).map(i64::from)
    })();
    ret(j, r)
}

fn socketpair(j: &mut Job, domain: i64, ty: i64, proto: i64, sv: u128) -> i64 {
    let r = (|| {
        mem(j, sv, 8)?;
        let (a, b) = net(j).borrow().sockets.borrow_mut().socketpair(int(domain), int(ty), int(proto))?;
        let na = adopt(j, a)?;
        let nb = match adopt(j, b) {
            Ok(n) => n,
            Err(e) => {
                let _ = j.fds.close(na);
                return Err(e);
            }
        };
        let out = mem(j, sv, 8)?;
        out[..4].copy_from_slice(&na.to_be_bytes());
        out[4..].copy_from_slice(&nb.to_be_bytes());
        Ok(0)
    })();
    ret(j, r)
}

fn bind(j: &mut Job, sd: i64, a: u128, len: i64, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let addr = addr_in(j, a, len, form)?;
        net(j).borrow().sockets.borrow_mut().bind(k, &addr)?;
        Ok(0)
    })();
    ret(j, r)
}

fn listen(j: &mut Job, sd: i64, backlog: i64) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        net(j).borrow().sockets.borrow().listen(k, int(backlog))?;
        Ok(0)
    })();
    ret(j, r)
}

fn accept(j: &mut Job, sd: i64, a: u128, lenp: u128, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let (nk, from) = net(j).borrow().sockets.borrow_mut().accept(k)?;
        let n = adopt(j, nk)?;
        addr_out(j, a, lenp, from.as_ref(), form)?;
        Ok(i64::from(n))
    })();
    ret(j, r)
}

#[allow(clippy::too_many_arguments)]
fn accept_and_recv(
    j: &mut Job,
    sd: i64,
    acc: u128,
    remote: u128,
    rlen: u128,
    local: u128,
    llen: u128,
    buf: u128,
    buflen: i64,
    form: AddrForm,
) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let n = len_of(buflen)?;
        let nt = net(j);
        let (nk, from) = nt.borrow().sockets.borrow_mut().accept(k)?;
        let nsd = adopt(j, nk)?;
        if !is_null(acc) {
            put_int(j, acc, nsd)?;
        }
        addr_out(j, remote, rlen, from.as_ref(), form)?;
        let me = nt.borrow().sockets.borrow().sockname(nk).ok();
        addr_out(j, local, llen, me.as_ref(), form)?;
        let got = match opt_mem(j, buf, n)? {
            Some(b) if n > 0 => nt.borrow().sockets.borrow().recv(nk, b, 0)?,
            _ => 0,
        };
        Ok(count(got))
    })();
    ret(j, r)
}

fn connect(j: &mut Job, sd: i64, a: u128, len: i64, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let addr = addr_in(j, a, len, form)?;
        net(j).borrow().sockets.borrow().connect(k, &addr)?;
        Ok(0)
    })();
    ret(j, r)
}

fn send(j: &mut Job, sd: i64, buf: u128, len: i64, flags: i64) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let b = mem(j, buf, len_of(len)?)?;
        net(j).borrow().sockets.borrow().send(k, b, int(flags)).map(count)
    })();
    ret(j, r)
}

#[allow(clippy::too_many_arguments)]
fn sendto(j: &mut Job, sd: i64, buf: u128, len: i64, flags: i64, to: u128, tolen: i64, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let b = mem(j, buf, len_of(len)?)?;
        let s = net(j);
        if is_null(to) {
            return s.borrow().sockets.borrow().send(k, b, int(flags)).map(count);
        }
        let addr = addr_in(j, to, tolen, form)?;
        s.borrow().sockets.borrow().sendto(k, b, int(flags), &addr).map(count)
    })();
    ret(j, r)
}

fn recv(j: &mut Job, sd: i64, buf: u128, len: i64, flags: i64) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let b = mem(j, buf, len_of(len)?)?;
        net(j).borrow().sockets.borrow().recv(k, b, int(flags)).map(count)
    })();
    ret(j, r)
}

#[allow(clippy::too_many_arguments)]
fn recvfrom(j: &mut Job, sd: i64, buf: u128, len: i64, flags: i64, from: u128, fromlen: u128, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let b = mem(j, buf, len_of(len)?)?;
        let (n, a) = net(j).borrow().sockets.borrow().recvfrom(k, b, int(flags))?;
        addr_out(j, from, fromlen, a.as_ref(), form)?;
        Ok(count(n))
    })();
    ret(j, r)
}

/// The buffers an `iovec` array names.
pub(crate) fn iovecs(j: &Job, iov: u128, n: i64) -> R<Vec<&'static mut [u8]>> {
    let n = len_of(n)?;
    if n > sockets::consts::IOV_MAX {
        return Err(Errno::EINVAL);
    }
    let arr = mem(j, iov, n * iovec::SIZE)?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let e = &arr[i * iovec::SIZE..(i + 1) * iovec::SIZE];
        let len = usize::try_from(sockets::layout::u32_at(e, iovec::LEN)).map_err(|_| Errno::EINVAL)?;
        let base = ptr_at(e, iovec::BASE);
        out.push(if len == 0 { &mut [][..] } else { mem(j, base, len)? });
    }
    Ok(out)
}

fn sendmsg(j: &mut Job, sd: i64, msg: u128, flags: i64, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let m = mem(j, msg, msghdr::SIZE)?;
        let name = ptr_at(m, msghdr::NAME);
        let namelen = i64::from(sockets::layout::i32_at(m, msghdr::NAMELEN));
        let to = if is_null(name) || namelen == 0 {
            None
        } else {
            Some(addr_in(j, name, namelen, form)?)
        };
        let bufs = iovecs(j, ptr_at(m, msghdr::IOV), i64::from(sockets::layout::i32_at(m, msghdr::IOVLEN)))?;
        let views: Vec<&[u8]> = bufs.iter().map(|b| &**b).collect();
        net(j).borrow().sockets.borrow().sendv(k, &views, int(flags), to.as_ref()).map(count)
    })();
    ret(j, r)
}

fn recvmsg(j: &mut Job, sd: i64, msg: u128, flags: i64, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let m = mem(j, msg, msghdr::SIZE)?;
        let mut bufs = iovecs(j, ptr_at(m, msghdr::IOV), i64::from(sockets::layout::i32_at(m, msghdr::IOVLEN)))?;
        let mut views: Vec<&mut [u8]> = bufs.iter_mut().map(|b| &mut **b).collect();
        let got = net(j).borrow().sockets.borrow().recvv(k, &mut views, int(flags))?;
        let name = ptr_at(m, msghdr::NAME);
        if !is_null(name) {
            let avail = usize::try_from(sockets::layout::i32_at(m, msghdr::NAMELEN)).unwrap_or(0);
            let n = match &got.from {
                Some(a) => a.encode(form, mem(j, name, avail.min(a.size()))?),
                None => 0,
            };
            sockets::layout::put_i32(m, msghdr::NAMELEN, i32::try_from(n).unwrap_or(0));
        }
        // No ancillary data is delivered (descriptor passing goes
        // through givedescriptor).
        sockets::layout::put_i32(m, msghdr::CONTROLLEN, 0);
        if form == AddrForm::Bsd44 {
            sockets::layout::put_i32(m, msghdr::FLAGS, got.flags);
        }
        Ok(count(got.bytes))
    })();
    ret(j, r)
}

fn shutdown(j: &mut Job, sd: i64, how: i64) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        net(j).borrow().sockets.borrow().shutdown(k, int(how))?;
        Ok(0)
    })();
    ret(j, r)
}

fn getsockopt(j: &mut Job, sd: i64, level: i64, name: i64, val: u128, lenp: u128) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let v = net(j).borrow().sockets.borrow().getopt(k, int(level), int(name))?;
        let avail = usize::try_from(get_int(j, lenp)?).map_err(|_| Errno::EINVAL)?;
        let n = avail.min(v.len());
        mem(j, val, n)?.copy_from_slice(&v[..n]);
        put_int(j, lenp, i32::try_from(n).unwrap_or(0))?;
        Ok(0)
    })();
    ret(j, r)
}

fn setsockopt(j: &mut Job, sd: i64, level: i64, name: i64, val: u128, len: i64) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let v = mem(j, val, len_of(len)?)?;
        net(j).borrow().sockets.borrow().setopt(k, int(level), int(name), v)?;
        Ok(0)
    })();
    ret(j, r)
}

fn sockname(j: &mut Job, sd: i64, a: u128, lenp: u128, peer: bool, form: AddrForm) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        if is_null(a) || is_null(lenp) {
            return Err(Errno::EFAULT);
        }
        let s = net(j);
        let addr = if peer {
            s.borrow().sockets.borrow().peername(k)?
        } else {
            s.borrow().sockets.borrow().sockname(k)?
        };
        addr_out(j, a, lenp, Some(&addr), form)?;
        Ok(0)
    })();
    ret(j, r)
}

/// `poll()` over job descriptors: sockets through the kernel, IFS
/// files always ready for what was asked.
pub(crate) fn poll_items(j: &mut Job, items: &[(i32, i16)], timeout: i32) -> R<Vec<i16>> {
    use sockets::consts::{POLLIN, POLLNVAL, POLLOUT, POLLPRI, POLLRDNORM, POLLWRNORM};
    let mut kernel_items = Vec::new();
    let mut slots = Vec::with_capacity(items.len());
    for (fd, ev) in items {
        match j.fds.kind(*fd) {
            Ok(Kind::Socket(k)) => {
                slots.push(Some(kernel_items.len()));
                kernel_items.push((k, *ev));
            }
            Ok(Kind::Ifs(_)) => slots.push(None),
            Err(_) => slots.push(Some(usize::MAX)),
        }
    }
    let any_ifs_ready = slots.iter().zip(items).any(|(s, (_, ev))| s.is_none() && *ev & (POLLIN | POLLOUT | POLLRDNORM | POLLWRNORM) != 0);
    let bad = slots.contains(&Some(usize::MAX));
    let wait = if any_ifs_ready || bad { 0 } else { timeout };
    let got = if kernel_items.is_empty() {
        if wait > 0 {
            std::thread::sleep(std::time::Duration::from_millis(u64::try_from(wait).unwrap_or(0)));
        }
        Vec::new()
    } else {
        net(j).borrow().sockets.borrow().poll(&kernel_items, wait)?
    };
    Ok(slots
        .iter()
        .zip(items)
        .map(|(s, (_, ev))| match s {
            Some(usize::MAX) => POLLNVAL,
            Some(i) => got.get(*i).copied().unwrap_or(0),
            None => *ev & (POLLIN | POLLOUT | POLLRDNORM | POLLWRNORM) & !POLLPRI,
        })
        .collect())
}

fn poll(j: &mut Job, fds: u128, nfds: i64, timeout: i64) -> i64 {
    let r = (|| {
        let n = usize::try_from(super::abi::uint(nfds)).map_err(|_| Errno::EINVAL)?;
        if n == 0 {
            poll_items(j, &[], int(timeout))?;
            return Ok(0);
        }
        let arr = mem(j, fds, n * 8)?;
        let items: Vec<(i32, i16)> = (0..n)
            .map(|i| {
                let p = PollFd::from_bytes(&arr[i * 8..i * 8 + 8]);
                (p.fd, p.events)
            })
            .collect();
        let rev = poll_items(j, &items, int(timeout))?;
        let mut ready = 0;
        for (i, r) in rev.iter().enumerate() {
            let mut p = PollFd::from_bytes(&arr[i * 8..i * 8 + 8]);
            p.revents = if p.fd < 0 { 0 } else { *r };
            if p.revents != 0 {
                ready += 1;
            }
            p.write(&mut arr[i * 8..i * 8 + 8]);
        }
        Ok(ready)
    })();
    ret(j, r)
}

fn givedescriptor(j: &mut Job, sd: i64, target: u128) -> i64 {
    let r = (|| {
        let k = kernel(j, int(sd))?;
        let t: [u8; 16] = mem(j, target, 16)?.try_into().expect("16 bytes");
        net(j).borrow().sockets.borrow_mut().give(k, &t)?;
        Ok(0)
    })();
    ret(j, r)
}

fn takedescriptor(j: &mut Job, source: u128) -> i64 {
    let r = (|| {
        let src: Option<[u8; 16]> = opt_mem(j, source, 16)?.map(|b| (&*b).try_into().expect("16 bytes"));
        let k = net(j).borrow().sockets.borrow_mut().take(src.as_ref())?;
        adopt(j, k).map(i64::from)
    })();
    ret(j, r)
}

fn gethostname(j: &mut Job, name: u128, len: i64) -> i64 {
    let r = (|| {
        let n = len_of(len)?;
        let d = net(j).borrow().resolver.domain().clone();
        let host = if d.host_name.is_empty() {
            j.ident.system.to_ascii_lowercase()
        } else if d.domain_name.is_empty() || d.host_name.contains('.') {
            d.host_name
        } else {
            format!("{}.{}", d.host_name, d.domain_name)
        };
        let mut b = ccsid::to_ebcdic(&host).unwrap_or_default();
        b.push(0);
        if b.len() > n {
            return Err(Errno::EINVAL);
        }
        mem(j, name, b.len())?.copy_from_slice(&b);
        Ok(0)
    })();
    ret(j, r)
}

macro_rules! forms {
    ($($f:ident($($a:ident: $t:ty),*) => $g:ident;)*) => {
        $(
            mod $f {
                use super::*;
                pub(super) fn b43(j: &mut Job, $($a: $t),*) -> i64 { $g(j, $($a,)* AddrForm::Bsd43) }
                pub(super) fn b44(j: &mut Job, $($a: $t),*) -> i64 { $g(j, $($a,)* AddrForm::Bsd44) }
            }
        )*
    };
}

forms! {
    f_bind(sd: i64, a: u128, l: i64) => bind;
    f_accept(sd: i64, a: u128, l: u128) => accept;
    f_accept_and_recv(sd: i64, acc: u128, ra: u128, rl: u128, la: u128, ll: u128, b: u128, bl: i64) => accept_and_recv;
    f_connect(sd: i64, a: u128, l: i64) => connect;
    f_sendto(sd: i64, b: u128, l: i64, f: i64, to: u128, tl: i64) => sendto;
    f_recvfrom(sd: i64, b: u128, l: i64, f: i64, fr: u128, fl: u128) => recvfrom;
    f_sendmsg(sd: i64, m: u128, f: i64) => sendmsg;
    f_recvmsg(sd: i64, m: u128, f: i64) => recvmsg;
    f_sockname(sd: i64, a: u128, l: u128) => sockname_mine;
    f_peername(sd: i64, a: u128, l: u128) => sockname_peer;
}

fn sockname_mine(j: &mut Job, sd: i64, a: u128, l: u128, form: AddrForm) -> i64 {
    sockname(j, sd, a, l, false, form)
}

fn sockname_peer(j: &mut Job, sd: i64, a: u128, l: u128, form: AddrForm) -> i64 {
    sockname(j, sd, a, l, true, form)
}

bound_c!(b_socket(d: i64, t: i64, p: i64) -> i64 = socket);
bound_c!(b_socketpair(d: i64, t: i64, p: i64, sv: u128) -> i64 = socketpair);
bound_c!(b_bind(sd: i64, a: u128, l: i64) -> i64 = f_bind::b43);
bound_c!(b_bind98(sd: i64, a: u128, l: i64) -> i64 = f_bind::b44);
bound_c!(b_listen(sd: i64, n: i64) -> i64 = listen);
bound_c!(b_accept(sd: i64, a: u128, l: u128) -> i64 = f_accept::b43);
bound_c!(b_accept98(sd: i64, a: u128, l: u128) -> i64 = f_accept::b44);
bound_c!(b_accept_and_recv(sd: i64, acc: u128, ra: u128, rl: u128, la: u128, ll: u128, b: u128, bl: i64) -> i64 = f_accept_and_recv::b43);
bound_c!(b_accept_and_recv98(sd: i64, acc: u128, ra: u128, rl: u128, la: u128, ll: u128, b: u128, bl: i64) -> i64 = f_accept_and_recv::b44);
bound_c!(b_connect(sd: i64, a: u128, l: i64) -> i64 = f_connect::b43);
bound_c!(b_connect98(sd: i64, a: u128, l: i64) -> i64 = f_connect::b44);
bound_c!(b_send(sd: i64, b: u128, l: i64, f: i64) -> i64 = send);
bound_c!(b_sendto(sd: i64, b: u128, l: i64, f: i64, to: u128, tl: i64) -> i64 = f_sendto::b43);
bound_c!(b_sendto98(sd: i64, b: u128, l: i64, f: i64, to: u128, tl: i64) -> i64 = f_sendto::b44);
bound_c!(b_sendmsg(sd: i64, m: u128, f: i64) -> i64 = f_sendmsg::b43);
bound_c!(b_sendmsg98(sd: i64, m: u128, f: i64) -> i64 = f_sendmsg::b44);
bound_c!(b_recv(sd: i64, b: u128, l: i64, f: i64) -> i64 = recv);
bound_c!(b_recvfrom(sd: i64, b: u128, l: i64, f: i64, fr: u128, fl: u128) -> i64 = f_recvfrom::b43);
bound_c!(b_recvfrom98(sd: i64, b: u128, l: i64, f: i64, fr: u128, fl: u128) -> i64 = f_recvfrom::b44);
bound_c!(b_recvmsg(sd: i64, m: u128, f: i64) -> i64 = f_recvmsg::b43);
bound_c!(b_recvmsg98(sd: i64, m: u128, f: i64) -> i64 = f_recvmsg::b44);
bound_c!(b_shutdown(sd: i64, h: i64) -> i64 = shutdown);
bound_c!(b_getsockopt(sd: i64, lv: i64, n: i64, v: u128, l: u128) -> i64 = getsockopt);
bound_c!(b_setsockopt(sd: i64, lv: i64, n: i64, v: u128, l: i64) -> i64 = setsockopt);
bound_c!(b_getsockname(sd: i64, a: u128, l: u128) -> i64 = f_sockname::b43);
bound_c!(b_getsockname98(sd: i64, a: u128, l: u128) -> i64 = f_sockname::b44);
bound_c!(b_getpeername(sd: i64, a: u128, l: u128) -> i64 = f_peername::b43);
bound_c!(b_getpeername98(sd: i64, a: u128, l: u128) -> i64 = f_peername::b44);
bound_c!(b_poll(f: u128, n: i64, t: i64) -> i64 = poll);
bound_c!(b_givedescriptor(sd: i64, t: u128) -> i64 = givedescriptor);
bound_c!(b_takedescriptor(s: u128) -> i64 = takedescriptor);
bound_c!(b_gethostname(n: u128, l: i64) -> i64 = gethostname);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("socket", at!(b_socket)),
        ("qso_socket98", at!(b_socket)),
        ("socketpair", at!(b_socketpair)),
        ("qso_socketpair98", at!(b_socketpair)),
        ("bind", at!(b_bind)),
        ("Rbind", at!(b_bind)),
        ("qso_bind98", at!(b_bind98)),
        ("qso_Rbind98", at!(b_bind98)),
        ("listen", at!(b_listen)),
        ("qso_listen98", at!(b_listen)),
        ("accept", at!(b_accept)),
        ("qso_accept98", at!(b_accept98)),
        ("accept_and_recv", at!(b_accept_and_recv)),
        ("qso_accept_and_recv98", at!(b_accept_and_recv98)),
        ("connect", at!(b_connect)),
        ("qso_connect98", at!(b_connect98)),
        ("send", at!(b_send)),
        ("qso_send98", at!(b_send)),
        ("sendto", at!(b_sendto)),
        ("qso_sendto98", at!(b_sendto98)),
        ("sendmsg", at!(b_sendmsg)),
        ("qso_sendmsg98", at!(b_sendmsg98)),
        ("recv", at!(b_recv)),
        ("qso_recv98", at!(b_recv)),
        ("recvfrom", at!(b_recvfrom)),
        ("qso_recvfrom98", at!(b_recvfrom98)),
        ("recvmsg", at!(b_recvmsg)),
        ("qso_recvmsg98", at!(b_recvmsg98)),
        ("shutdown", at!(b_shutdown)),
        ("qso_shutdown98", at!(b_shutdown)),
        ("getsockopt", at!(b_getsockopt)),
        ("qso_getsockopt98", at!(b_getsockopt)),
        ("setsockopt", at!(b_setsockopt)),
        ("qso_setsockopt98", at!(b_setsockopt)),
        ("getsockname", at!(b_getsockname)),
        ("qso_getsockname98", at!(b_getsockname98)),
        ("getpeername", at!(b_getpeername)),
        ("qso_getpeername98", at!(b_getpeername98)),
        ("poll", at!(b_poll)),
        ("givedescriptor", at!(b_givedescriptor)),
        ("takedescriptor", at!(b_takedescriptor)),
        ("gethostname", at!(b_gethostname)),
    ]
}

