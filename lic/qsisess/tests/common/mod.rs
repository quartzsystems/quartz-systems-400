// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the tests share: raw frames to play a misbehaving peer, and
//! reading what a session sent.

#![allow(dead_code)]

use qsipart::frame::{Frame, Reader};
use qsisess::wire::{self, kind};
use qsisess::{Incoming, Reply, Request};

/// The frames in `bytes`.
pub fn frames(bytes: &[u8]) -> Vec<Frame> {
    let mut r = Reader::new();
    let mut out = Vec::new();
    for c in bytes.chunks(1024) {
        r.push(c);
        out.extend(r.frames());
    }
    assert_eq!(r.skipped(), 0);
    out
}

/// The single-frame replies in `bytes`.
pub fn replies(bytes: &[u8]) -> Vec<Reply> {
    frames(bytes)
        .into_iter()
        .filter(|f| f.kind == kind::REPLY)
        .map(|f| wire::read::<Reply>(&f.body[4..]).expect("a reply"))
        .collect()
}

/// A request's frames.
pub fn request(id: u32, user: &str, family: u16, op: u16, body: Vec<u8>) -> Vec<u8> {
    let r = Request {
        id,
        user: user.into(),
        family,
        op,
        body,
    };
    wire::message_frames(kind::REQUEST, &ipc::encode(&r))
}

/// The close among `items`: its identifier and whether the peer closed.
pub fn closed(items: &[Incoming]) -> Option<(Option<String>, bool)> {
    items.iter().find_map(|i| match i {
        Incoming::Closed { msgid, by_peer, .. } => Some((msgid.clone(), *by_peer)),
        _ => None,
    })
}

/// The refusal identifier of the reply to `id` among `items`.
pub fn refused(items: &[Incoming], id: u32) -> Option<String> {
    items.iter().find_map(|i| match i {
        Incoming::Reply(r) if r.id == id => r.refusal().map(|(m, _)| m.to_owned()),
        _ => None,
    })
}
