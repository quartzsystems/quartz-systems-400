// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSOSRV2`: address conversion and the resolver. Results the program
//! reads through a returned pointer (`hostent`, `servent`, `inet_ntoa`'s
//! text, `addrinfo` lists) live in storage the job's [`Net`] owns, at
//! addresses that stay valid; the `_r` forms fill the caller's.
//!
//! [`Net`]: super::Net

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use mi::{FatPtr, Job};
use sockets::consts::{AF_INET, AF_INET6, EAI_MEMORY, EAI_SYSTEM, INADDR_NONE, NETDB_MAX_HOST_NAME_LENGTH};
use sockets::layout::netdb::{self as images, Image};
use sockets::layout::{HOSTENT, HOSTENT_DATA, i32_at};
use sockets::resolver::{HostError, Hints, gai_strerror, hstrerror};
use sockets::{AddrForm, Errno, SockAddr};

use super::abi::{bound_c, cstr, fail, fat, int, is_null, mem, opt_cstr, put_ptr, raw, ret, uint};
use super::net;

type R<T> = Result<T, Errno>;

/// Copy an image to storage at `ptr` and fill its pointer slots.
fn place(j: &Job, dst: &mut [u8], ptr: FatPtr, im: &Image) -> R<()> {
    let n = im.bytes.len();
    if dst.len() < n {
        return Err(Errno::ERANGE);
    }
    dst[..n].copy_from_slice(&im.bytes);
    for &(slot, target) in &im.ptrs {
        let p = j.spaces.add(ptr, i64::try_from(target).expect("small")).map_err(|_| Errno::EFAULT)?;
        put_ptr(dst, slot, p);
    }
    Ok(())
}

/// An image in one of the job's fixed buffers; the pointer to it.
fn fixed_image(j: &mut Job, what: &'static str, size: usize, im: &Image) -> R<FatPtr> {
    let n = net(j);
    let mut nb = n.borrow_mut();
    let st = nb.fixed(j, what, size.max(im.bytes.len()));
    let ptr = st.ptr();
    let bytes = st.bytes();
    bytes.fill(0);
    place(j, bytes, ptr, im)?;
    Ok(ptr)
}

/// `inet_addr` parsing: `a.b.c.d`, `a.b.c`, `a.b` and `a`, each part
/// decimal, octal (`0` first) or hex (`0x` first).
fn parse_inet(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut v = Vec::with_capacity(parts.len());
    for p in &parts {
        let n = if let Some(h) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
            u32::from_str_radix(h, 16).ok()?
        } else if p.len() > 1 && p.starts_with('0') {
            u32::from_str_radix(&p[1..], 8).ok()?
        } else {
            p.parse::<u32>().ok()?
        };
        v.push(n);
    }
    let (last, head) = v.split_last()?;
    let mut out = 0u32;
    for (i, h) in head.iter().enumerate() {
        if *h > 255 {
            return None;
        }
        out |= h << (24 - 8 * i);
    }
    let room = 32 - 8 * head.len();
    if room < 32 && *last >> room != 0 {
        return None;
    }
    Some(out | last)
}

fn inet_addr(j: &mut Job, cp: u128) -> i64 {
    match cstr(j, cp, 64) {
        Ok(s) => i64::from(parse_inet(&s).unwrap_or(INADDR_NONE)),
        Err(_) => i64::from(INADDR_NONE),
    }
}

fn ntoa_text(v: u32) -> String {
    Ipv4Addr::from(v).to_string()
}

fn put_cstr(j: &Job, p: u128, n: usize, s: &str) -> R<()> {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
    b.push(0);
    if b.len() > n {
        return Err(Errno::ENOSPC);
    }
    mem(j, p, b.len())?.copy_from_slice(&b);
    Ok(())
}

fn inet_ntoa(j: &mut Job, v: i64) -> u128 {
    let mut b = ccsid::to_ebcdic(&ntoa_text(uint(v))).unwrap_or_default();
    b.push(0);
    let im = Image {
        bytes: b,
        ptrs: Vec::new(),
    };
    fixed_image(j, "ntoa", 16, &im).map_or(0, raw)
}

fn inet_ntoa_r(j: &mut Job, v: i64, buf: u128, len: i64) -> i64 {
    let n = usize::try_from(int(len)).unwrap_or(0);
    let r = put_cstr(j, buf, n, &ntoa_text(uint(v))).map(|()| 0);
    ret(j, r)
}

fn inet_pton(j: &mut Job, af: i64, src: u128, dst: u128) -> i64 {
    let r = (|| {
        let s = cstr(j, src, 64)?;
        match int(af) {
            AF_INET => match s.parse::<Ipv4Addr>() {
                Ok(a) => {
                    mem(j, dst, 4)?.copy_from_slice(&a.octets());
                    Ok(1)
                }
                Err(_) => Ok(0),
            },
            AF_INET6 => match s.parse::<Ipv6Addr>() {
                Ok(a) => {
                    mem(j, dst, 16)?.copy_from_slice(&a.octets());
                    Ok(1)
                }
                Err(_) => Ok(0),
            },
            _ => Err(Errno::EAFNOSUPPORT),
        }
    })();
    ret(j, r)
}

fn inet_ntop(j: &mut Job, af: i64, src: u128, dst: u128, size: i64) -> u128 {
    let r = (|| {
        let s = match int(af) {
            AF_INET => {
                let b = mem(j, src, 4)?;
                Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string()
            }
            AF_INET6 => {
                let b: [u8; 16] = (&*mem(j, src, 16)?).try_into().expect("16");
                Ipv6Addr::from(b).to_string()
            }
            _ => return Err(Errno::EAFNOSUPPORT),
        };
        put_cstr(j, dst, usize::try_from(uint(size)).unwrap_or(0), &s)?;
        Ok(dst)
    })();
    match r {
        Ok(p) => p,
        Err(e) => {
            fail(j, e);
            0
        }
    }
}

fn host_failed(j: &mut Job, e: HostError) -> u128 {
    j.h_errno.set(e.code());
    0
}

fn host_image(h: &sockets::resolver::Host) -> Image {
    images::hostent(&h.name, &h.aliases, h.family, &h.addr_bytes())
}

fn gethostbyname(j: &mut Job, name: u128) -> u128 {
    let Ok(name) = cstr(j, name, NETDB_MAX_HOST_NAME_LENGTH + 1) else {
        return host_failed(j, HostError::NoRecovery);
    };
    let found = net(j).borrow_mut().resolver.by_name(&name, AF_INET);
    match found {
        Ok(h) => match fixed_image(j, "hostent", HOSTENT.size + HOSTENT_DATA.size, &host_image(&h)) {
            Ok(p) => raw(p),
            Err(_) => host_failed(j, HostError::NoRecovery),
        },
        Err(e) => host_failed(j, e),
    }
}

/// Fill a caller's `hostent` and `hostent_data`.
fn host_r(j: &mut Job, found: Result<sockets::resolver::Host, HostError>, result: u128, data: u128) -> i64 {
    let h = match found {
        Ok(h) => h,
        Err(e) => {
            j.h_errno.set(e.code());
            return -1;
        }
    };
    let r = (|| {
        let split = host_image(&h).split(HOSTENT.size);
        let head = mem(j, result, HOSTENT.size)?;
        let buf = mem(j, data, HOSTENT_DATA.size)?;
        if split.data.len() > buf.len() {
            return Err(Errno::ERANGE);
        }
        let dp = fat(data);
        head.copy_from_slice(&split.head);
        buf[..split.data.len()].copy_from_slice(&split.data);
        for (slots, dst) in [(&split.head_ptrs, &mut *head), (&split.data_ptrs, &mut *buf)] {
            for &(slot, target) in slots {
                let p = j.spaces.add(dp, i64::try_from(target).expect("small")).map_err(|_| Errno::EFAULT)?;
                put_ptr(dst, slot, p);
            }
        }
        Ok(0)
    })();
    match r {
        Ok(v) => v,
        Err(e) => {
            j.h_errno.set(HostError::NoRecovery.code());
            fail(j, e)
        }
    }
}

fn gethostbyname_r(j: &mut Job, name: u128, result: u128, data: u128) -> i64 {
    let found = match cstr(j, name, NETDB_MAX_HOST_NAME_LENGTH + 1) {
        Ok(n) => net(j).borrow_mut().resolver.by_name(&n, AF_INET),
        Err(_) => Err(HostError::NoRecovery),
    };
    host_r(j, found, result, data)
}

fn addr_arg(j: &Job, a: u128, len: i64, ty: i64) -> Result<IpAddr, HostError> {
    let n = usize::try_from(int(len)).map_err(|_| HostError::NoRecovery)?;
    let b = mem(j, a, n).map_err(|_| HostError::NoRecovery)?;
    match (int(ty), n) {
        (AF_INET, 4) => Ok(IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3]))),
        (AF_INET6, 16) => Ok(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&*b).expect("16")))),
        _ => Err(HostError::NoRecovery),
    }
}

fn gethostbyaddr(j: &mut Job, a: u128, len: i64, ty: i64) -> u128 {
    let found = match addr_arg(j, a, len, ty) {
        Ok(ip) => net(j).borrow_mut().resolver.by_addr(ip),
        Err(e) => Err(e),
    };
    match found {
        Ok(h) => match fixed_image(j, "hostent", HOSTENT.size + HOSTENT_DATA.size, &host_image(&h)) {
            Ok(p) => raw(p),
            Err(_) => host_failed(j, HostError::NoRecovery),
        },
        Err(e) => host_failed(j, e),
    }
}

fn gethostbyaddr_r(j: &mut Job, a: u128, len: i64, ty: i64, result: u128, data: u128) -> i64 {
    let found = match addr_arg(j, a, len, ty) {
        Ok(ip) => net(j).borrow_mut().resolver.by_addr(ip),
        Err(e) => Err(e),
    };
    host_r(j, found, result, data)
}

fn getaddrinfo(j: &mut Job, node: u128, service: u128, hints: u128, res: u128, form: AddrForm) -> i64 {
    let r = (|| {
        let node = opt_cstr(j, node, NETDB_MAX_HOST_NAME_LENGTH + 1).map_err(|_| EAI_SYSTEM)?;
        let service = opt_cstr(j, service, 64).map_err(|_| EAI_SYSTEM)?;
        let h = match super::abi::opt_mem(j, hints, 96).map_err(|_| EAI_SYSTEM)? {
            Some(b) => Hints {
                flags: i32_at(b, 0),
                family: i32_at(b, 4),
                socktype: i32_at(b, 8),
                protocol: i32_at(b, 12),
            },
            None => Hints::default(),
        };
        mem(j, res, 16).map_err(|_| EAI_SYSTEM)?;
        let n = net(j);
        let list = n.borrow_mut().resolver.getaddrinfo(node.as_deref(), service.as_deref(), &h)?;
        let im = images::addrinfo(&list, form);
        let mut nb = n.borrow_mut();
        let mut st = super::Net::keep_list(j, &im.bytes);
        let base = st.ptr();
        place(j, st.bytes(), base, &im).map_err(|_| EAI_MEMORY)?;
        let native = j.spaces.deref(base).map_err(|_| EAI_MEMORY)?;
        nb.lists.insert(native, st);
        put_ptr(mem(j, res, 16).map_err(|_| EAI_SYSTEM)?, 0, base);
        Ok(0)
    })();
    match r {
        Ok(v) => v,
        Err(EAI_SYSTEM) => {
            j.errno.set(Errno::EFAULT.code());
            i64::from(EAI_SYSTEM)
        }
        Err(e) => i64::from(e),
    }
}

fn freeaddrinfo(j: &mut Job, ai: u128) -> i64 {
    if is_null(ai) {
        return 0;
    }
    if let Ok(native) = j.spaces.deref(fat(ai)) {
        let n = net(j);
        n.borrow_mut().free_list(j, native);
    }
    0
}

#[allow(clippy::too_many_arguments)]
fn getnameinfo(
    j: &mut Job,
    sa: u128,
    salen: i64,
    host: u128,
    hostlen: i64,
    serv: u128,
    servlen: i64,
    flags: i64,
    form: AddrForm,
) -> i64 {
    let r: Result<i64, i32> = (|| {
        let n = usize::try_from(uint(salen)).map_err(|_| sockets::consts::EAI_FAIL)?;
        let b = mem(j, sa, n.min(128)).map_err(|_| EAI_SYSTEM)?;
        let addr = SockAddr::decode(b, form).map_err(|_| sockets::consts::EAI_FAMILY)?;
        let (h, s) = net(j).borrow_mut().resolver.getnameinfo(&addr, int(flags))?;
        let hl = usize::try_from(uint(hostlen)).unwrap_or(0);
        let sl = usize::try_from(uint(servlen)).unwrap_or(0);
        if !is_null(host) && hl > 0 {
            put_cstr(j, host, hl, &h).map_err(|_| sockets::consts::EAI_OVERFLOW)?;
        }
        if !is_null(serv) && sl > 0 {
            put_cstr(j, serv, sl, &s).map_err(|_| sockets::consts::EAI_OVERFLOW)?;
        }
        Ok(0)
    })();
    r.unwrap_or_else(i64::from)
}

fn gai_strerror_p(j: &mut Job, code: i64) -> u128 {
    let c = int(code);
    let n = net(j);
    raw(n.borrow_mut().text(j, "gai", c, gai_strerror(c)))
}

fn hstrerror_p(j: &mut Job, code: i64) -> u128 {
    let c = int(code);
    let n = net(j);
    raw(n.borrow_mut().text(j, "h", c, hstrerror(c)))
}

fn h_errno_ptr(j: &mut Job) -> u128 {
    raw(j.h_errno.pointer(&mut j.spaces))
}

fn res_init(j: &mut Job) -> i64 {
    let n = net(j);
    let mut nb = n.borrow_mut();
    nb.resolver.init();
    let o = nb.resolver.options() | sockets::consts::RES_INIT;
    nb.resolver.set_options(o);
    0
}

fn res_close(_j: &mut Job) -> i64 {
    0
}

fn res_common(j: &mut Job, name: u128, class: i64, ty: i64, ans: u128, len: i64, search: bool) -> i64 {
    let Ok(name) = cstr(j, name, sockets::consts::MAXDNAME) else {
        j.h_errno.set(HostError::NoRecovery.code());
        return -1;
    };
    let (class, ty) = (u16::try_from(int(class)).unwrap_or(0), u16::try_from(int(ty)).unwrap_or(0));
    let got = {
        let n = net(j);
        let mut nb = n.borrow_mut();
        if search {
            nb.resolver.search(&name, class, ty)
        } else {
            nb.resolver.query(&name, class, ty)
        }
    };
    match got {
        Ok(b) => {
            let room = usize::try_from(int(len)).unwrap_or(0).min(b.len());
            match mem(j, ans, room) {
                Ok(out) => {
                    out.copy_from_slice(&b[..room]);
                    i64::try_from(b.len()).unwrap_or(i64::MAX)
                }
                Err(e) => fail(j, e),
            }
        }
        Err(e) => {
            j.h_errno.set(e.code());
            -1
        }
    }
}

fn res_query(j: &mut Job, name: u128, class: i64, ty: i64, ans: u128, len: i64) -> i64 {
    res_common(j, name, class, ty, ans, len, false)
}

fn res_search(j: &mut Job, name: u128, class: i64, ty: i64, ans: u128, len: i64) -> i64 {
    res_common(j, name, class, ty, ans, len, true)
}

fn serv_result(j: &mut Job, s: Option<sockets::resolver::Service>) -> u128 {
    match s {
        Some(s) => {
            let im = images::servent(&s.name, &s.aliases, s.port, &s.protocol);
            fixed_image(j, "servent", 1024, &im).map_or(0, raw)
        }
        None => 0,
    }
}

fn getservbyname(j: &mut Job, name: u128, proto: u128) -> u128 {
    let (Ok(name), Ok(proto)) = (cstr(j, name, 64), opt_cstr(j, proto, 16)) else {
        return 0;
    };
    let s = net(j).borrow().resolver.services().by_name(&name, proto.as_deref());
    serv_result(j, s)
}

fn getservbyport(j: &mut Job, port: i64, proto: u128) -> u128 {
    let Ok(proto) = opt_cstr(j, proto, 16) else {
        return 0;
    };
    let port = u16::try_from(int(port)).unwrap_or(0);
    let s = net(j).borrow().resolver.services().by_port(port, proto.as_deref());
    serv_result(j, s)
}

fn getaddrinfo43(j: &mut Job, n: u128, s: u128, h: u128, r: u128) -> i64 {
    getaddrinfo(j, n, s, h, r, AddrForm::Bsd43)
}

fn getaddrinfo44(j: &mut Job, n: u128, s: u128, h: u128, r: u128) -> i64 {
    getaddrinfo(j, n, s, h, r, AddrForm::Bsd44)
}

#[allow(clippy::too_many_arguments)]
fn getnameinfo43(j: &mut Job, sa: u128, sl: i64, h: u128, hl: i64, s: u128, svl: i64, f: i64) -> i64 {
    getnameinfo(j, sa, sl, h, hl, s, svl, f, AddrForm::Bsd43)
}

#[allow(clippy::too_many_arguments)]
fn getnameinfo44(j: &mut Job, sa: u128, sl: i64, h: u128, hl: i64, s: u128, svl: i64, f: i64) -> i64 {
    getnameinfo(j, sa, sl, h, hl, s, svl, f, AddrForm::Bsd44)
}

bound_c!(b_inet_addr(cp: u128) -> i64 = inet_addr);
bound_c!(b_inet_ntoa(v: i64) -> u128 = inet_ntoa);
bound_c!(b_inet_ntoa_r(v: i64, b: u128, l: i64) -> i64 = inet_ntoa_r);
bound_c!(b_inet_pton(af: i64, s: u128, d: u128) -> i64 = inet_pton);
bound_c!(b_inet_ntop(af: i64, s: u128, d: u128, n: i64) -> u128 = inet_ntop);
bound_c!(b_gethostbyname(n: u128) -> u128 = gethostbyname);
bound_c!(b_gethostbyname_r(n: u128, r: u128, d: u128) -> i64 = gethostbyname_r);
bound_c!(b_gethostbyaddr(a: u128, l: i64, t: i64) -> u128 = gethostbyaddr);
bound_c!(b_gethostbyaddr_r(a: u128, l: i64, t: i64, r: u128, d: u128) -> i64 = gethostbyaddr_r);
bound_c!(b_getaddrinfo(n: u128, s: u128, h: u128, r: u128) -> i64 = getaddrinfo43);
bound_c!(b_getaddrinfo98(n: u128, s: u128, h: u128, r: u128) -> i64 = getaddrinfo44);
bound_c!(b_freeaddrinfo(a: u128) -> i64 = freeaddrinfo);
bound_c!(b_getnameinfo(sa: u128, sl: i64, h: u128, hl: i64, s: u128, svl: i64, f: i64) -> i64 = getnameinfo43);
bound_c!(b_getnameinfo98(sa: u128, sl: i64, h: u128, hl: i64, s: u128, svl: i64, f: i64) -> i64 = getnameinfo44);
bound_c!(b_gai_strerror(c: i64) -> u128 = gai_strerror_p);
bound_c!(b_hstrerror(c: i64) -> u128 = hstrerror_p);
bound_c!(b_h_errno() -> u128 = h_errno_ptr);
bound_c!(b_res_init() -> i64 = res_init);
bound_c!(b_res_close() -> i64 = res_close);
bound_c!(b_res_query(n: u128, c: i64, t: i64, a: u128, l: i64) -> i64 = res_query);
bound_c!(b_res_search(n: u128, c: i64, t: i64, a: u128, l: i64) -> i64 = res_search);
bound_c!(b_getservbyname(n: u128, p: u128) -> u128 = getservbyname);
bound_c!(b_getservbyport(port: i64, p: u128) -> u128 = getservbyport);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("inet_addr", at!(b_inet_addr)),
        ("qso_inet_addr98", at!(b_inet_addr)),
        ("inet_ntoa", at!(b_inet_ntoa)),
        ("inet_ntoa_r", at!(b_inet_ntoa_r)),
        ("inet_pton", at!(b_inet_pton)),
        ("inet_ntop", at!(b_inet_ntop)),
        ("gethostbyname", at!(b_gethostbyname)),
        ("qso_gethostbyname98", at!(b_gethostbyname)),
        ("gethostbyname_r", at!(b_gethostbyname_r)),
        ("qso_gethostbyname_r98", at!(b_gethostbyname_r)),
        ("gethostbyaddr", at!(b_gethostbyaddr)),
        ("qso_gethostbyaddr98", at!(b_gethostbyaddr)),
        ("gethostbyaddr_r", at!(b_gethostbyaddr_r)),
        ("qso_gethostbyaddr_r98", at!(b_gethostbyaddr_r)),
        ("getaddrinfo", at!(b_getaddrinfo)),
        ("qso_getaddrinfo98", at!(b_getaddrinfo98)),
        ("freeaddrinfo", at!(b_freeaddrinfo)),
        ("getnameinfo", at!(b_getnameinfo)),
        ("qso_getnameinfo98", at!(b_getnameinfo98)),
        ("gai_strerror", at!(b_gai_strerror)),
        ("hstrerror", at!(b_hstrerror)),
        ("__h_errno", at!(b_h_errno)),
        ("res_init", at!(b_res_init)),
        ("res_close", at!(b_res_close)),
        ("res_query", at!(b_res_query)),
        ("res_search", at!(b_res_search)),
        ("getservbyname", at!(b_getservbyname)),
        ("qso_getservbyname98", at!(b_getservbyname)),
        ("getservbyport", at!(b_getservbyport)),
        ("qso_getservbyport98", at!(b_getservbyport)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inet_addr_forms() {
        assert_eq!(parse_inet("127.0.0.1"), Some(0x7f00_0001));
        assert_eq!(parse_inet("10.1"), Some(0x0a00_0001));
        assert_eq!(parse_inet("0x7f.1"), Some(0x7f00_0001));
        assert_eq!(parse_inet("010.0.0.1"), Some(0x0800_0001));
        assert_eq!(parse_inet("1.2.3.256"), None);
        assert_eq!(parse_inet("1.2.3.4.5"), None);
        assert_eq!(parse_inet("x"), None);
        assert_eq!(parse_inet("4294967295"), Some(u32::MAX));
    }
}
