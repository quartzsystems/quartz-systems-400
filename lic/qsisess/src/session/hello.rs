// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The peer's hello: the older version is spoken, only the capabilities
//! both have are used.

use qsi::{Hello, PeerKind, msgid};

use super::{Role, Session, State};
use crate::Incoming;

impl Session {
    pub(super) fn on_hello(&mut self, body: &[u8]) {
        let Ok(peer) = qsi::message::body::<Hello>(0, 0, body) else {
            return self.fail(msgid::BAD_FRAME, "a hello that does not read");
        };
        if peer.version == 0 {
            return self.fail(msgid::NO_VERSION, "the peer speaks no version");
        }
        if let Err(why) = peer.check() {
            return self.fail(msgid::BAD_FRAME, why);
        }
        if (peer.kind == PeerKind::Console) == (self.role == Role::Console) {
            return self.fail(msgid::WRONG_PEER, "the peer is the same kind of side");
        }
        let agreed = self.ours.agree(&peer);
        self.state = State::Open;
        self.peer = Some((peer.clone(), agreed));
        self.incoming.push_back(Incoming::Ready { peer, agreed });
    }
}
