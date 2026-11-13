// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ONC RPC version 2 (RFC 5531; docs/NFS.md section 3).
//!
//! - [`msg`]: call and reply messages over the generated `rpc` types.
//! - [`record`]: record marking over TCP.
//! - [`auth`]: `AUTH_NONE` and `AUTH_SYS`; other flavors are refused.
//! - [`server`]: the [`Program`] trait, the [`Dispatcher`], the TCP and
//!   UDP serve functions.
//! - [`drc`]: the duplicate request cache.
//! - [`client`]: [`RpcClient`] over TCP and UDP.
//!
//! A program over a generated procedure table:
//!
//! ```
//! use nfs::rpc::{Answer, CallContext, Program, decode_args, encode_res};
//! use nfs::types::nsm::{SM_PROG, SM_PROG::SM_VERS, res, sm_stat_res};
//! use nfs::xdr::Decoder;
//!
//! struct Monitor;
//!
//! impl Program for Monitor {
//!     fn program(&self) -> u32 {
//!         SM_PROG::NUMBER
//!     }
//!     fn versions(&self) -> &[u32] {
//!         &[SM_VERS::NUMBER]
//!     }
//!     fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
//!         use SM_VERS::{Args, Res};
//!         let res = match decode_args::<Args>(ctx, d)? {
//!             Args::SM_NULL => Res::SM_NULL,
//!             Args::SM_STAT(_) => Res::SM_STAT(sm_stat_res { res_stat: res::stat_succ, state: 3 }),
//!             // (A real monitor answers its other procedures here.)
//!             _ => return Err(nfs::rpc::AcceptError::ProcUnavail),
//!         };
//!         encode_res(&res)
//!     }
//! }
//! ```
//!
//! And a client calling it: `client.call_proc::<SM_VERS::Args,
//! SM_VERS::Res>(&Credential::None, &Args::SM_STAT(name))`, `SM_VERS`
//! being `nfs::types::nsm::SM_PROG::SM_VERS`.

pub mod auth;
pub mod client;
pub mod drc;
pub mod msg;
pub mod record;
pub mod server;

pub use auth::Credential;
pub use client::{ClientConfig, Proto, RpcClient, RpcError};
pub use drc::DuplicateCache;
pub use msg::{AcceptError, Denied};
pub use server::{
    Answer, CallContext, Dispatcher, LocalProgram, Program, Transport, decode_args, encode_res,
    serve_datagram, serve_stream, serve_tcp,
};
