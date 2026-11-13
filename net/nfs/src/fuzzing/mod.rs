// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Entry points for the fuzz targets (docs/NFS.md section 11). Each
//! takes any bytes and must return without a panic or a hang. The
//! decode-then-encode entry points also check that what decodes
//! encodes back to the bytes it came from, and decodes again to the
//! same value; a difference is a bug and panics.
//!
//! Wave 2 adds its servers to [`rpc`] and its own entry points beside
//! these; [`roundtrip`] and [`roundtrip_proc`] serve any generated type.

use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use crate::rpc::msg::encode_call;
use crate::rpc::record::{MAX_RECORD, RecordBuf};
use crate::rpc::{
    Answer, CallContext, Credential, Dispatcher, DuplicateCache, Program, Transport, auth,
};
use crate::rpcb::{Binder, rpcbtab};
use crate::types::rpc::call_body;
use crate::types::{mount, nfs3, nfs4, nlm, nsm};
use crate::xdr::{Decoder, Encoder, ProcBody, Xdr};

/// Decodes a `T` from `data`; if it decodes, encodes it, checks the
/// bytes are those consumed, and decodes them again to the same value.
pub fn roundtrip<T: Xdr + PartialEq + Debug>(data: &[u8]) {
    let mut d = Decoder::new(data);
    let Ok(v) = T::decode(&mut d) else { return };
    let used = data.get(..d.position()).unwrap_or_default();
    let mut e = Encoder::new();
    v.encode(&mut e).expect("a decoded value encodes");
    assert_eq!(e.as_bytes(), used, "re-encoding differs");
    let again = T::decode(&mut Decoder::new(e.as_bytes())).expect("re-encoded bytes decode");
    assert_eq!(again, v);
}

/// [`roundtrip`] for the body of `procedure` in a version's `Args` or `Res`.
pub fn roundtrip_proc<P: ProcBody + PartialEq + Debug>(procedure: u32, data: &[u8]) {
    let mut d = Decoder::new(data);
    let Ok(Some(v)) = P::decode_body(procedure, &mut d) else {
        return;
    };
    let used = data.get(..d.position()).unwrap_or_default();
    let mut e = Encoder::new();
    v.encode_body(&mut e).expect("a decoded value encodes");
    assert_eq!(e.as_bytes(), used, "re-encoding differs");
    assert_eq!(v.procedure(), procedure);
}

/// The first byte as a procedure number, and the rest.
fn split(data: &[u8]) -> (u32, &[u8]) {
    match data.split_first() {
        Some((p, rest)) => (u32::from(*p), rest),
        None => (0, data),
    }
}

/// A program that answers its arguments back, for the dispatcher.
struct Echo;

impl Program for Echo {
    fn program(&self) -> u32 {
        400_100
    }
    fn versions(&self) -> &[u32] {
        &[1, 3]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        match ctx.procedure {
            0 => Ok(Some(Vec::new())),
            1 => {
                let b = d
                    .get_opaque(1024)
                    .map_err(|_| crate::rpc::AcceptError::GarbageArgs)?;
                let mut e = Encoder::new();
                e.put_opaque(&b, 1024)
                    .map_err(|_| crate::rpc::AcceptError::SystemErr)?;
                Ok(Some(e.into_bytes()))
            }
            2 => Ok(None),
            _ => Err(crate::rpc::AcceptError::ProcUnavail),
        }
    }
}

fn dispatcher() -> &'static Dispatcher {
    static D: OnceLock<Dispatcher> = OnceLock::new();
    D.get_or_init(|| {
        let mut d = Dispatcher::new();
        d.add(Arc::new(Echo));
        d.add(Arc::new(Binder::default()));
        d.set_cache(Arc::new(DuplicateCache::new(
            std::time::Duration::from_secs(60),
            64,
            |p, _, n| p == 400_100 && n == 1,
        )));
        d
    })
}

fn peer() -> SocketAddr {
    SocketAddr::from(([10, 0, 2, 2], 1023))
}

/// Record marking, call parsing and the dispatcher (with an echo
/// program and the binder), over TCP and UDP.
pub fn rpc(data: &[u8]) {
    let d = dispatcher();
    let _ = d.handle(data, peer(), Transport::Udp);
    let mut rb = RecordBuf::new(MAX_RECORD);
    for chunk in data.chunks(97) {
        rb.push(chunk);
        while let Ok(Some(m)) = rb.next_record() {
            let _ = d.handle(&m, peer(), Transport::Tcp);
        }
    }
    let _ = crate::rpc::record::read_record(&mut &data[..], MAX_RECORD);
    let _ = crate::rpc::msg::parse_reply(data);
    if let Ok(c) = crate::rpc::msg::parse_call(data) {
        let _ = Credential::from_opaque(&c.body.cred);
    }
}

/// The binder end to end (the bytes after two selector bytes are the
/// arguments of a call from the local host), and `/etc/RPCBTAB`.
pub fn rpcb(data: &[u8]) {
    if let Ok(t) = rpcbtab::decode(data) {
        let b = rpcbtab::encode(&t).expect("a table read can be written");
        assert_eq!(rpcbtab::decode(&b).as_ref(), Ok(&t), "RPCBTAB differs");
    }
    let [v, p, args @ ..] = data else { return };
    let body = call_body {
        rpcvers: 2,
        prog: 100_000,
        vers: 2 + u32::from(*v % 3),
        proc: u32::from(*p % 14),
        cred: Credential::sys(b"fuzz", 0, 0, &[])
            .to_opaque()
            .unwrap_or_else(|_| auth::none()),
        verf: auth::none(),
    };
    let Ok(m) = encode_call(7, &body, args) else {
        return;
    };
    let binder = Binder::default();
    let mut d = Dispatcher::new();
    d.add(Arc::new(binder));
    let local = SocketAddr::from(([127, 0, 0, 1], 700));
    for t in [Transport::Tcp, Transport::Udp] {
        if let Some(r) = d.handle(&m, local, t) {
            let _ = crate::rpc::client::results(&r);
        }
    }
}

/// MOUNT versions 1 and 3: every procedure's arguments and results.
pub fn mount(data: &[u8]) {
    let (p, rest) = split(data);
    roundtrip_proc::<mount::MOUNT_PROGRAM::MOUNT_V1::Args>(p, rest);
    roundtrip_proc::<mount::MOUNT_PROGRAM::MOUNT_V1::Res>(p, rest);
    roundtrip_proc::<mount::MOUNT_PROGRAM::MOUNT_V3::Args>(p, rest);
    roundtrip_proc::<mount::MOUNT_PROGRAM::MOUNT_V3::Res>(p, rest);
}

/// NLM versions 1, 3 and 4 and NSM: every procedure's arguments and results.
pub fn nlm(data: &[u8]) {
    let (p, rest) = split(data);
    roundtrip_proc::<nlm::NLM_PROG::NLM_VERS::Args>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM_VERS::Res>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM_VERSX::Args>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM_VERSX::Res>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM4_VERS::Args>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM4_VERS::Res>(p, rest);
    roundtrip_proc::<nlm::NLM_PROG::NLM_SM::Args>(p, rest);
    nsm(data);
}

/// NSM: every procedure's arguments and results.
pub fn nsm(data: &[u8]) {
    let (p, rest) = split(data);
    roundtrip_proc::<nsm::SM_PROG::SM_VERS::Args>(p, rest);
    roundtrip_proc::<nsm::SM_PROG::SM_VERS::Res>(p, rest);
    roundtrip::<nsm::status>(data);
}

/// NFS version 3: every procedure's arguments and results.
pub fn nfs3(data: &[u8]) {
    let (p, rest) = split(data);
    roundtrip_proc::<nfs3::NFS_PROGRAM::NFS_V3::Args>(p, rest);
    roundtrip_proc::<nfs3::NFS_PROGRAM::NFS_V3::Res>(p, rest);
}

/// NFS version 4: the compound's arguments and results, and the callback's.
pub fn nfs4(data: &[u8]) {
    roundtrip::<nfs4::COMPOUND4args>(data);
    roundtrip::<nfs4::COMPOUND4res>(data);
    roundtrip::<nfs4::CB_COMPOUND4args>(data);
    roundtrip::<nfs4::CB_COMPOUND4res>(data);
}
