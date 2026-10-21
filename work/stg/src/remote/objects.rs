// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Programs, user spaces and message descriptions from the service.

use std::collections::HashMap;
use std::sync::Arc;

use ile::{PgmKind, ProgramObject};
use mi::{ObjectSource, UserSpaceSource};
use msgf::{MsgDesc, MsgLookup};

use crate::Shared;
use crate::proto::Client;

/// Programs and service programs.
pub struct RemoteObjects {
    link: Shared,
}

impl RemoteObjects {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

impl ObjectSource for RemoteObjects {
    fn program(
        &mut self,
        name: &str,
        lib: &str,
        kind: PgmKind,
    ) -> Option<(Arc<ProgramObject>, String)> {
        let k = match kind {
            PgmKind::Pgm => 0,
            PgmKind::SrvPgm => 1,
        };
        let (bytes, pname, plib) = self
            .link
            .borrow_mut()
            .with(|l| l.program(name.into(), lib.into(), k))?;
        let mut p = ProgramObject::decode(&bytes).ok()?;
        p.name = pname;
        Some((Arc::new(p), plib))
    }

    fn resolve(&mut self, name: &str, lib: &str, obj_type: &str) -> Option<u64> {
        self.link
            .borrow_mut()
            .with(|l| l.resolve(name.into(), lib.into(), obj_type.into()))
    }

    fn by_id(&mut self, id: u64) -> Option<(String, String)> {
        self.link.borrow_mut().with(|l| l.by_id(id))
    }
}

/// User spaces.
pub struct RemoteSpaces {
    link: Shared,
}

impl RemoteSpaces {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self { link }
    }
}

/// The `&'static` message identifier the trait wants.
fn static_id(id: &str) -> &'static str {
    match id {
        "CPF9870" => "CPF9870",
        "CPF9810" => "CPF9810",
        "CPF9801" => "CPF9801",
        "CPF9838" => "CPF9838",
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

impl UserSpaceSource for RemoteSpaces {
    fn load(&mut self, name: &str, lib: &str) -> Option<Vec<u8>> {
        self.link
            .borrow_mut()
            .with(|l| l.us_load(name.into(), lib.into()))
    }

    fn store(&mut self, name: &str, lib: &str, data: &[u8]) -> bool {
        self.link
            .borrow_mut()
            .with(|l| l.us_store(name.into(), lib.into(), data.to_vec()))
    }

    fn create(
        &mut self,
        name: &str,
        lib: &str,
        size: usize,
        init: u8,
        text: &str,
    ) -> Result<(), &'static str> {
        self.link
            .borrow_mut()
            .with(|l| l.us_create(name.into(), lib.into(), size as u64, init, text.into()))
            .map_err(|e| static_id(&e))
    }

    fn delete(&mut self, name: &str, lib: &str) -> bool {
        self.link
            .borrow_mut()
            .with(|l| l.us_delete(name.into(), lib.into()))
    }

    fn library_of(&mut self, name: &str, lib: &str) -> Option<String> {
        self.link
            .borrow_mut()
            .with(|l| l.us_library_of(name.into(), lib.into()))
    }
}

/// Message descriptions, remembered once fetched: they change only
/// through `CHGMSGD`, which a running job does not wait for.
pub struct RemoteMsgs {
    link: Shared,
    cache: HashMap<(String, String, String), Option<MsgDesc>>,
}

impl RemoteMsgs {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self {
            link,
            cache: HashMap::new(),
        }
    }
}

impl MsgLookup for RemoteMsgs {
    fn describe(&mut self, msgid: &str, msgf: &str, lib: &str) -> Option<MsgDesc> {
        let key = (msgid.to_string(), msgf.to_string(), lib.to_string());
        if let Some(d) = self.cache.get(&key) {
            return d.clone();
        }
        let d = self
            .link
            .borrow_mut()
            .with(|l| l.describe(msgid.into(), msgf.into(), lib.into()));
        self.cache.insert(key, d.clone());
        d
    }
}
