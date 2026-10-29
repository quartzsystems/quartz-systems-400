// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Frames: the header, the template, the LL/CP parameters.

pub mod header;
pub mod param;

pub use header::{HEADER_LEN, Header, frame_len};
pub use param::Param;

use crate::Error;

/// One frame split into its parts. Encoding gives back the bytes it
/// was decoded from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The header as sent (its length is not recomputed).
    pub header: Header,
    /// The template: `header.template_len` bytes after the header.
    pub template: Vec<u8>,
    /// The parameters after the template.
    pub params: Vec<Param>,
    /// Bytes after the last whole parameter (rare: data that is not LL/CP).
    pub tail: Vec<u8>,
}

impl Frame {
    /// Decode one whole frame.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let header = Header::decode(b)?;
        if header.length as usize != b.len() {
            return Err(Error::Length {
                said: header.length as usize,
                have: b.len(),
            });
        }
        let t_end = HEADER_LEN + usize::from(header.template_len);
        if t_end > b.len() {
            return Err(Error::Short {
                what: "template",
                need: t_end,
                have: b.len(),
            });
        }
        let (params, tail) = param::split(&b[t_end..]);
        Ok(Self {
            header,
            template: b[HEADER_LEN..t_end].to_vec(),
            params,
            tail: tail.to_vec(),
        })
    }

    /// A frame with a correct length and template length.
    pub fn new(server_id: u16, req_rep_id: u16, template: Vec<u8>, params: Vec<Param>) -> Self {
        let mut f = Self {
            header: Header::new(server_id, req_rep_id, 0, 0),
            template,
            params,
            tail: Vec::new(),
        };
        f.fix_lengths();
        f
    }

    /// Recompute the total and template lengths.
    pub fn fix_lengths(&mut self) {
        let body = self.template.len()
            + self.params.iter().map(Param::len).sum::<usize>()
            + self.tail.len();
        self.header.length = u32::try_from(HEADER_LEN + body).unwrap_or(u32::MAX);
        self.header.template_len = u16::try_from(self.template.len()).unwrap_or(u16::MAX);
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.header.length as usize);
        self.header.encode(&mut out);
        out.extend_from_slice(&self.template);
        param::encode_all(&self.params, &mut out);
        out.extend_from_slice(&self.tail);
        out
    }

    /// The first parameter with code point `cp`.
    pub fn param(&self, cp: u16) -> Option<&Param> {
        param::find(&self.params, cp)
    }
}
