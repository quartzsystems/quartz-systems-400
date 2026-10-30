// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SFTP subsystem, protocol version 3, over an [`IfsClient`]: every
//! path the IFS resolves works, `/QSYS.LIB` included. Relative paths
//! start at the user's home directory.

pub mod names;
pub mod proto;
mod server;

use std::io::{Read, Write};

use ifs::client::IfsClient;

pub use server::Server;

/// The largest request accepted.
pub const MAX_PACKET: usize = 256 * 1024 + 1024;

/// Serve one SFTP session until the client closes its input.
pub fn serve(ifs: &dyn IfsClient, home: &str, input: &mut dyn Read, output: &mut dyn Write) {
    let mut s = Server::new(ifs, home);
    loop {
        let mut len = [0u8; 4];
        if input.read_exact(&mut len).is_err() {
            break;
        }
        let n = u32::from_be_bytes(len) as usize;
        if n == 0 || n > MAX_PACKET {
            break;
        }
        let mut body = vec![0u8; n];
        if input.read_exact(&mut body).is_err() {
            break;
        }
        let Some(reply) = s.handle(&body) else {
            break;
        };
        let mut out = u32::try_from(reply.len()).unwrap_or(0).to_be_bytes().to_vec();
        out.extend_from_slice(&reply);
        if output.write_all(&out).and_then(|()| output.flush()).is_err() {
            break;
        }
    }
    s.close_all();
}
