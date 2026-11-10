// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lease states and the rules for breaking and granting them across
//! owners: the SMB2 bits and MS-FSA 2.1.4.12's table, the same rules the
//! SMB library applies among one job's opens.

/// Read caching (`SMB2_LEASE_READ_CACHING`).
pub const R: u8 = 0x1;
/// Handle caching (`SMB2_LEASE_HANDLE_CACHING`).
pub const H: u8 = 0x2;
/// Write caching (`SMB2_LEASE_WRITE_CACHING`).
pub const W: u8 = 0x4;
/// Every caching bit.
pub const RWH: u8 = R | H | W;

/// What another owner's operation does to a file's leases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// An open of the file; `overwrite` when it truncates.
    Open {
        /// The open truncates the file.
        overwrite: bool,
    },
    /// A write, a truncation or a byte-range lock.
    Write,
    /// An open refused by a share mode while handles are cached.
    Sharing,
    /// A rename of the file or over it, or its removal.
    Handle,
}

/// The state a lease in `state` breaks to for `trigger`, and whether the
/// operation waits for the holder's acknowledgment; `None` when the
/// lease keeps its state.
#[must_use]
pub fn break_to(trigger: Trigger, state: u8) -> Option<(u8, bool)> {
    let (to, wait) = match trigger {
        Trigger::Open { overwrite: false } => (state & !W, true),
        Trigger::Open { overwrite: true } | Trigger::Write => (0, state & W != 0),
        Trigger::Sharing | Trigger::Handle => (state & !H, true),
    };
    (to != state).then_some((to, wait))
}

/// Whether breaking a lease in `state` needs the holder's
/// acknowledgment: anything beyond read caching does.
#[must_use]
pub fn needs_ack(state: u8) -> bool {
    state & !R != 0
}

/// The state a request asks for: only R, RH, RW and RWH are leases;
/// anything without R asks for none.
#[must_use]
pub fn requested(bits: u8) -> u8 {
    let s = bits & RWH;
    if s & R == 0 { 0 } else { s }
}

/// The states to try for a request, best first: as asked, without W,
/// then without W and H.
pub fn candidates(want: u8) -> impl Iterator<Item = u8> {
    let no_w = want & !W;
    let no_wh = no_w & !H;
    let mut v = vec![want];
    if want & W != 0 {
        v.push(no_w);
    }
    if no_w & H != 0 {
        v.push(no_wh);
    }
    v.into_iter().filter(|s| *s != 0)
}

/// In a holder's report: the lease is being granted and keeps the state
/// it was reserved with.
pub const KEEP: u8 = 0x80;
