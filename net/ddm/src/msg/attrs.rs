// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `EXCSAT` and its reply data `EXCSATRD`: the partners' names, release
//! levels and the managers each supports at which level.

use crate::cp::{cmd, mgr, param, rpy};
use crate::wire::{Object, text};

/// Server attributes, as sent and as answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attributes {
    /// `EXTNAM`: the job or process.
    pub extnam: Option<String>,
    /// `SRVNAM`: the system.
    pub srvnam: Option<String>,
    /// `SRVCLSNM`: the product class (`QAS` for IBM i).
    pub srvclsnm: Option<String>,
    /// `SRVRLSLV`: the product release.
    pub srvrlslv: Option<String>,
    /// `MGRLVLLS`: manager code point and level.
    pub managers: Vec<(u16, u16)>,
}

impl Attributes {
    /// The level of a manager, if listed.
    pub fn level(&self, manager: u16) -> Option<u16> {
        self.managers
            .iter()
            .find(|(m, _)| *m == manager)
            .map(|&(_, l)| l)
    }

    /// The managers a QS/400 requester asks for.
    pub fn requester_managers() -> Vec<(u16, u16)> {
        vec![
            (mgr::AGENT, 7),
            (mgr::SQLAM, 7),
            (mgr::RDB, 7),
            (mgr::SECMGR, 7),
            (mgr::CMNTCPIP, 5),
        ]
    }

    /// As an `EXCSAT` command.
    pub fn to_excsat(&self) -> Object {
        Object::coll(cmd::EXCSAT, self.items())
    }

    /// As `EXCSATRD` reply data.
    pub fn to_excsatrd(&self) -> Object {
        Object::coll(rpy::EXCSATRD, self.items())
    }

    fn items(&self) -> Vec<Object> {
        let mut v = Vec::new();
        if let Some(x) = &self.extnam {
            v.push(Object::chars(param::EXTNAM, x));
        }
        if !self.managers.is_empty() {
            let mut d = Vec::with_capacity(self.managers.len() * 4);
            for (m, l) in &self.managers {
                d.extend(m.to_be_bytes());
                d.extend(l.to_be_bytes());
            }
            v.push(Object::data(param::MGRLVLLS, d));
        }
        if let Some(x) = &self.srvclsnm {
            v.push(Object::chars(param::SRVCLSNM, x));
        }
        if let Some(x) = &self.srvnam {
            v.push(Object::chars(param::SRVNAM, x));
        }
        if let Some(x) = &self.srvrlslv {
            v.push(Object::chars(param::SRVRLSLV, x));
        }
        v
    }

    /// From `EXCSAT` or `EXCSATRD`.
    pub fn from_object(o: &Object) -> Self {
        let managers = o
            .get(param::MGRLVLLS)
            .map(|m| {
                m.bytes()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| (u16::from_be_bytes([c[0], c[1]]), u16::from_be_bytes([c[2], c[3]])))
                    .collect()
            })
            .unwrap_or_default();
        let t = |cp| o.get(cp).map(|x| text(x.bytes()));
        Self {
            extnam: t(param::EXTNAM),
            srvnam: t(param::SRVNAM),
            srvclsnm: t(param::SRVCLSNM),
            srvrlslv: t(param::SRVRLSLV),
            managers,
        }
    }
}
