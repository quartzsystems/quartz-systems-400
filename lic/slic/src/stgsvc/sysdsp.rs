// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the system displays share between jobs: group jobs (`CHGGRPA`,
//! `TFRGRPJOB`, `ENDGRPJOB`) with their group data area (`*GDA`), and
//! the jobs at each display device as the TN5250 session holding it
//! reports them (secondary jobs, Display Work Station User). The state
//! lives as long as the service: a group ends with its last job.

use std::cell::RefCell;
use std::collections::BTreeMap;

use stg::proto::{GroupInfo, WsJob};

use crate::ipl::system::System;

/// The size of a group data area.
pub const GDA_LEN: usize = 512;

/// One group.
#[derive(Clone, Debug)]
struct Group {
    /// Members: group job name, job number, text.
    members: Vec<(String, u32, String)>,
    /// The `*GDA`, EBCDIC blanks to start.
    gda: Vec<u8>,
}

#[derive(Default)]
struct State {
    groups: BTreeMap<u32, Group>,
    next: u32,
    /// Jobs at a device: number, user, name, side.
    devices: BTreeMap<String, Vec<WsJob>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { next: 1, ..State::default() });
}

/// A group job name: up to ten characters, a letter first.
fn valid_name(name: &str) -> bool {
    let mut c = name.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || "$#@".contains(f))
        && name.len() <= 10
        && name.chars().all(|ch| ch.is_ascii_alphanumeric() || "$#@_".contains(ch))
}

fn group_of(s: &State, number: u32) -> Option<u32> {
    s.groups
        .iter()
        .find(|(_, g)| g.members.iter().any(|m| m.1 == number))
        .map(|(id, _)| *id)
}

/// `CHGGRPA GRPJOB(name) GRPJOBTXT(text)` for job `number`.
pub fn grp_chg(number: u32, name: &str, text: &str) -> Result<u32, String> {
    let name = name.trim().to_ascii_uppercase();
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        if name == "*NONE" {
            let Some(id) = group_of(&s, number) else {
                return Ok(0);
            };
            if let Some(g) = s.groups.get_mut(&id)
                && g.members.len() > 1
            {
                return Err(format!(
                    "CPF1310 Group job cannot leave the group while {} other jobs are in it.",
                    g.members.len() - 1
                ));
            }
            s.groups.remove(&id);
            return Ok(0);
        }
        if !valid_name(&name) {
            return Err(format!("CPF1305 Group job name {name} is not valid."));
        }
        if let Some(id) = group_of(&s, number) {
            let g = s.groups.get_mut(&id).expect("group");
            if g.members.iter().any(|m| m.0 == name && m.1 != number) {
                return Err(format!("CPF1306 Group job {name} already exists in the group."));
            }
            if let Some(m) = g.members.iter_mut().find(|m| m.1 == number) {
                m.0.clone_from(&name);
                if !text.is_empty() && text != "*SAME" {
                    m.2 = text.to_string();
                }
            }
            return Ok(id);
        }
        let id = s.next;
        s.next += 1;
        s.groups.insert(
            id,
            Group {
                members: vec![(name, number, text.to_string())],
                gda: vec![ccsid::SPACE; GDA_LEN],
            },
        );
        Ok(id)
    })
}

/// The group of job `number`, the jobs that are gone dropped.
pub fn grp_info(system: &System, number: u32) -> Option<GroupInfo> {
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        for g in s.groups.values_mut() {
            g.members.retain(|m| m.1 == number || system.jobs.get(m.1).is_some());
        }
        let id = group_of(&s, number)?;
        let g = &s.groups[&id];
        let me = g.members.iter().find(|m| m.1 == number)?.0.clone();
        Some((id, me, g.members.clone()))
    })
}

/// A job started by `TFRGRPJOB` joins its group.
pub fn grp_join(group: u32, name: &str, text: &str, number: u32) -> Result<(), String> {
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        let g = s
            .groups
            .get_mut(&group)
            .ok_or_else(|| "CPF1309 The group no longer exists.".to_string())?;
        if g.members.iter().any(|m| m.0 == name) {
            return Err(format!("CPF1306 Group job {name} already exists in the group."));
        }
        g.members.push((name.to_string(), number, text.to_string()));
        Ok(())
    })
}

/// Job `number` left its group; a group without members goes.
pub fn grp_leave(number: u32) {
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        for g in s.groups.values_mut() {
            g.members.retain(|m| m.1 != number);
        }
        s.groups.retain(|_, g| !g.members.is_empty());
    });
}

/// The `*GDA` of job `number`'s group.
pub fn gda_read(number: u32) -> Result<Vec<u8>, String> {
    STATE.with(|st| {
        let s = st.borrow();
        let id = group_of(&s, number).ok_or_else(not_group)?;
        Ok(s.groups[&id].gda.clone())
    })
}

/// Change the `*GDA` of job `number`'s group from `start` (1-based).
pub fn gda_write(number: u32, start: u32, bytes: &[u8]) -> Result<(), String> {
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        let id = group_of(&s, number).ok_or_else(not_group)?;
        let at = (start.max(1) - 1) as usize;
        if at + bytes.len() > GDA_LEN {
            return Err("CPF1088 The value does not fit the group data area.".into());
        }
        s.groups.get_mut(&id).expect("group").gda[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    })
}

fn not_group() -> String {
    "CPF1311 The job is not a group job: *GDA does not exist.".into()
}

/// The session holding `device` reports its jobs.
pub fn ws_set(device: &str, jobs: Vec<WsJob>) {
    STATE.with(|st| {
        let mut s = st.borrow_mut();
        if jobs.is_empty() {
            s.devices.remove(device);
        } else {
            s.devices.insert(device.to_string(), jobs);
        }
    });
}

/// The jobs at `device` that are still active.
pub fn ws_get(system: &System, device: &str) -> Vec<WsJob> {
    STATE.with(|st| {
        st.borrow()
            .devices
            .get(device)
            .map(|v| {
                v.iter()
                    .filter(|j| system.jobs.get(j.0).is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_their_area() {
        let g = grp_chg(900, "GRP1", "first").unwrap();
        assert_eq!(grp_chg(900, "GRP1", "*SAME").unwrap(), g);
        grp_join(g, "GRP2", "second", 901).unwrap();
        assert!(grp_join(g, "GRP2", "", 902).is_err());
        gda_write(901, 3, &[0xC1, 0xC2]).unwrap();
        let gda = gda_read(900).unwrap();
        assert_eq!(gda.len(), GDA_LEN);
        assert_eq!(&gda[..4], &[0x40, 0x40, 0xC1, 0xC2]);
        assert!(gda_write(900, 512, &[1, 2]).is_err());
        assert!(grp_chg(901, "*NONE", "").is_err());
        grp_leave(901);
        assert_eq!(grp_chg(900, "*NONE", "").unwrap(), 0);
        assert!(gda_read(900).is_err());
        assert!(grp_chg(903, "1BAD", "").is_err());
    }
}
