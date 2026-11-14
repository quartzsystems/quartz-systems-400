// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory publishing over the link (docs/DIRSRV.md §10.1): the
//! configuration for `QGLDPUBE` and `QGLDPUBA` (`dirpub_config`, 1570),
//! an agent's changes after its cursor with its status reported
//! (`dirpub_events`, 1571), and every object of an agent
//! (`dirpub_list`, 1572). An agent is named `users` or `printers`.

use ipc::{wire_enum, wire_struct};

/// What a publishing job is given: `publish.conf`'s text, each agent's
/// password in the clear, and the system name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PubConfig {
    /// The file's text; empty when there is none.
    pub text: String,
    /// The agents' passwords, by agent name.
    pub passwords: Vec<(String, String)>,
    /// The system name.
    pub system: String,
}

wire_struct!(PubConfig {
    text,
    passwords,
    system,
});

/// A place in the service's change log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PubCursor {
    /// The service's epoch.
    pub epoch: u64,
    /// The last change seen.
    pub seq: u64,
}

wire_struct!(PubCursor { epoch, seq });

/// A profile or a printer device to publish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PubObject {
    /// A user profile.
    User {
        /// The profile name.
        name: String,
        /// Its text.
        text: String,
        /// Its SMTP address, when there is one.
        mail: Option<String>,
    },
    /// A printer device with an output queue.
    Printer {
        /// The device name.
        device: String,
        /// Its text.
        text: String,
        /// Its output queue, `LIB/NAME`.
        outq: String,
        /// A LAN printer's manufacturer type and model.
        make_model: Option<String>,
        /// Where the system takes print for it.
        uri: Option<String>,
    },
}

wire_enum!(PubObject {
    0 => User { name, text, mail },
    1 => Printer { device, text, outq, make_model, uri },
});

/// One change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PubChange {
    /// A new object.
    Created {
        /// It.
        object: PubObject,
    },
    /// An object whose published values changed.
    Changed {
        /// It, as it is now.
        object: PubObject,
    },
    /// An object gone.
    Deleted {
        /// Its name.
        name: String,
    },
}

wire_enum!(PubChange {
    0 => Created { object },
    1 => Changed { object },
    2 => Deleted { name },
});

/// What `dirpub_events` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PubChanges {
    /// The cursor to ask from next.
    pub next: PubCursor,
    /// The log no longer reaches the cursor: list again.
    pub resync: bool,
    /// The changes, oldest first.
    pub changes: Vec<PubChange>,
}

wire_struct!(PubChanges {
    next,
    resync,
    changes,
});

/// What `dirpub_list` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PubListing {
    /// The changes the list includes.
    pub cursor: PubCursor,
    /// Every object of the agent.
    pub objects: Vec<PubObject>,
}

wire_struct!(PubListing { cursor, objects });

/// An agent's status, as `dirsrv::publish::Status`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PubStatus {
    /// Whether it publishes.
    pub enabled: bool,
    /// Whether it holds a connection to the target.
    pub connected: bool,
    /// When the target last took everything pending.
    pub last_success: Option<u64>,
    /// The last error and when.
    pub last_error: Option<(u64, String)>,
    /// Changes still to make.
    pub pending: u32,
    /// Entries published.
    pub published: u32,
    /// Entries the target refused.
    pub errors: u32,
    /// When the target is tried again.
    pub next_try: Option<u64>,
}

wire_struct!(PubStatus {
    enabled,
    connected,
    last_success,
    last_error,
    pending,
    published,
    errors,
    next_try,
});

#[cfg(test)]
mod tests {
    use super::*;
    use ipc::Wire;

    fn round<T: Wire + PartialEq + std::fmt::Debug>(v: &T) {
        let mut b = Vec::new();
        v.put(&mut b);
        let mut d = ipc::Dec::new(&b);
        assert_eq!(&T::get(&mut d).unwrap(), v);
    }

    #[test]
    fn round_trips() {
        round(&PubConfig {
            text: "version: 1\n".into(),
            passwords: vec![("users".into(), "pw".into())],
            system: "QSHQ400".into(),
        });
        round(&PubChanges {
            next: PubCursor { epoch: 9, seq: 4 },
            resync: false,
            changes: vec![
                PubChange::Created {
                    object: PubObject::User {
                        name: "A".into(),
                        text: "Ann".into(),
                        mail: None,
                    },
                },
                PubChange::Changed {
                    object: PubObject::Printer {
                        device: "PRT01".into(),
                        text: String::new(),
                        outq: "QUSRSYS/PRT01".into(),
                        make_model: Some("*HP4".into()),
                        uri: Some("lpd://h/PRT01".into()),
                    },
                },
                PubChange::Deleted { name: "B".into() },
            ],
        });
        round(&PubListing::default());
        round(&PubStatus {
            enabled: true,
            last_error: Some((5, "down".into())),
            next_try: Some(9),
            ..PubStatus::default()
        });
    }
}
