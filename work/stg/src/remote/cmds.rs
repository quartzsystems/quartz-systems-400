// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Command definitions from the service: the analyzer's source in a
//! job process. Definitions are cached per job and the cache is
//! dropped when the job creates or changes a command.

use std::collections::HashMap;

use cmd::analyze::CmdSource;
use cmd::payload::CmdObject;
use cmd::{CmdError, Diag};

use crate::Shared;
use crate::proto::Client;

/// A definition found (or not) for a name and library.
type Found = Option<(CmdObject, String)>;

/// `*CMD` objects over a link.
pub struct RemoteCmds {
    link: Shared,
    cache: HashMap<(String, Option<String>), Found>,
}

impl RemoteCmds {
    /// Over a link.
    #[must_use]
    pub fn new(link: Shared) -> Self {
        Self {
            link,
            cache: HashMap::new(),
        }
    }

    /// Forget what was looked up: a command changed.
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl CmdSource for RemoteCmds {
    fn lookup(
        &mut self,
        name: &str,
        lib: Option<&str>,
    ) -> cmd::Result<Option<(CmdObject, String)>> {
        let key = (name.to_string(), lib.map(str::to_string));
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit.clone());
        }
        let r = self
            .link
            .borrow_mut()
            .with(|l| l.cmd_lookup(name.into(), lib.map(str::to_string)));
        let found = match r {
            Ok(None) => None,
            Ok(Some((bytes, found_lib))) => Some((CmdObject::decode(&bytes)?, found_lib)),
            Err(msgid) => {
                return Err(CmdError::from(
                    Diag::new(&msgid).data(&[(name, 10), (lib.unwrap_or("*LIBL"), 10)]),
                ));
            }
        };
        self.cache.insert(key, found.clone());
        Ok(found)
    }
}
