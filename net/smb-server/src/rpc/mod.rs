//! DCE/RPC over the named pipes of IPC$ (C706 chapter 12, MS-RPCE 2.1.1.2).

use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) mod association;
pub(crate) mod ndr;
pub(crate) mod pdu;
pub(crate) mod pipe;
pub(crate) mod spoolss;
pub(crate) mod srvsvc;

/// The pipe IPC$ serves, as a CREATE names it.
pub(crate) const SRVSVC_PIPE: &str = "srvsvc";

/// QS/400: the print spooler's pipe (MS-RPRN 2.1).
pub(crate) const SPOOLSS_PIPE: &str = "spoolss";

/// QS/400: the interface a pipe serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Interface {
    Srvsvc,
    Spoolss,
}

impl Interface {
    /// The interface of the pipe `name`, compared ignoring ASCII case.
    pub(crate) fn of_pipe(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case(SRVSVC_PIPE) {
            Some(Self::Srvsvc)
        } else if name.eq_ignore_ascii_case(SPOOLSS_PIPE) {
            Some(Self::Spoolss)
        } else {
            None
        }
    }

    pub(crate) fn syntax(self) -> pdu::SyntaxId {
        match self {
            Self::Srvsvc => pdu::SRVSVC,
            Self::Spoolss => pdu::SPOOLSS,
        }
    }

    /// The secondary address of a bind_ack (MS-RPCE 3.3.1.5.5).
    pub(crate) fn secondary_address(self) -> &'static str {
        match self {
            Self::Srvsvc => "\\PIPE\\srvsvc",
            Self::Spoolss => "\\PIPE\\spoolss",
        }
    }

    /// Largest request stub one call may reassemble: spoolss carries
    /// print data in RpcWritePrinter.
    pub(crate) fn max_request_stub(self) -> usize {
        match self {
            Self::Srvsvc => 64 << 10,
            Self::Spoolss => (1 << 20) + (4 << 10),
        }
    }
}

/// Source of association group ids for binds that ask for a new group.
static NEXT_ASSOC_GROUP: AtomicU32 = AtomicU32::new(1);

/// A new association group id, never 0 (C706 12.6.3.6).
pub(crate) fn new_assoc_group() -> u32 {
    loop {
        let id = NEXT_ASSOC_GROUP.fetch_add(1, Ordering::Relaxed);
        if id != 0 {
            return id;
        }
    }
}
