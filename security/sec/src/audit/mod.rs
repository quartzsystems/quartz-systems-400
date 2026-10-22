// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security auditing. Code that does something auditable records an
//! [`Event`]; the storage service takes the request's events
//! ([`take`]), keeps the ones the auditing values select ([`Control`])
//! and writes them to `QSYS/QAUDJRN` as journal entries of code `T`
//! ([`write`]), in a transaction of their own so a request that failed
//! is audited too. `docs/SECURITY.md` §7 has the catalog and layouts.

pub mod force;
pub mod kind;
pub mod layout;
pub mod write;

use std::cell::RefCell;

pub use kind::Kind;

use crate::rec::Rec;

/// Something to audit. Fields are named by their suffix (`ONAM` for
/// `AFONAM`); the job and user come from the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The entry type.
    pub kind: Kind,
    /// Field suffix and value.
    pub fields: Vec<(&'static str, String)>,
    /// The user to charge, when not the job's (a sign-on that failed).
    pub user: Option<String>,
}

impl Event {
    /// An event with no fields yet.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            fields: Vec::new(),
            user: None,
        }
    }

    /// Set field `suffix`.
    #[must_use]
    pub fn set(mut self, suffix: &'static str, value: impl Into<String>) -> Self {
        self.fields.push((suffix, value.into()));
        self
    }

    /// The object fields (`ONAM`, `OLIB`, `OTYP`).
    #[must_use]
    pub fn object(self, name: &str, lib: &str, type_name: &str) -> Self {
        self.set("ONAM", name)
            .set("OLIB", lib)
            .set("OTYP", type_name)
    }

    /// Charged to `user`.
    #[must_use]
    pub fn by(mut self, user: &str) -> Self {
        self.user = Some(user.into());
        self
    }

    /// A field's value, empty when unset.
    #[must_use]
    pub fn get(&self, suffix: &str) -> &str {
        self.fields
            .iter()
            .find(|(s, _)| *s == suffix)
            .map_or("", |(_, v)| v.as_str())
    }

    /// The entry-specific data in the type's layout.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let Some(l) = layout::of(self.kind) else {
            return Vec::new();
        };
        let mut r = Rec::new(l);
        for f in l.fields {
            let suffix = &f.name[2..];
            r.set_text(f.name, self.get(suffix));
        }
        r.buf
    }

    /// Back from the entry-specific data.
    #[must_use]
    pub fn decode(kind: Kind, data: &[u8]) -> Option<Self> {
        let l = layout::of(kind)?;
        let r = Rec::read(l, data).ok()?;
        let mut e = Self::new(kind);
        for f in l.fields {
            let suffix: &'static str = &f.name[2..];
            e.fields.push((suffix, r.text(f.name)));
        }
        Some(e)
    }
}

thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

/// Record an event for the request in progress.
pub fn record(e: Event) {
    QUEUE.with(|q| q.borrow_mut().push(e));
}

/// The events recorded since the last call.
#[must_use]
pub fn take() -> Vec<Event> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// The auditing values in force.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Control {
    /// `QAUDCTL`.
    pub ctl: Vec<String>,
    /// `QAUDLVL` and `QAUDLVL2`, expanded.
    pub lvl: Vec<String>,
}

impl Control {
    /// From the system values; `get` answers a value's text.
    pub fn from_sysvals(get: &dyn Fn(&str) -> String) -> Self {
        let words = |n: &str| -> Vec<String> {
            get(n)
                .split_whitespace()
                .map(str::to_ascii_uppercase)
                .collect()
        };
        let mut lvl = words("QAUDLVL");
        if lvl.iter().any(|v| v == "*AUDLVL2") {
            lvl.extend(words("QAUDLVL2"));
        }
        Self {
            ctl: words("QAUDCTL"),
            lvl: kind::expand(&lvl),
        }
    }

    fn has(&self, v: &str) -> bool {
        self.ctl.iter().any(|c| c == v)
    }

    /// Whether anything is audited.
    #[must_use]
    pub fn on(&self) -> bool {
        self.has("*AUDLVL") || self.has("*OBJAUD")
    }

    /// Whether object auditing (`ZC`, `ZR`) is on.
    #[must_use]
    pub fn objaud(&self) -> bool {
        self.has("*OBJAUD")
    }

    /// Whether `QTEMP` objects are left out.
    #[must_use]
    pub fn noqtemp(&self) -> bool {
        self.has("*NOQTEMP")
    }

    /// Whether an event is written for a user whose `AUDLVL` is
    /// `user_lvl`. Object auditing events were recorded only when the
    /// object's (or the user's) `OBJAUD` asked for them.
    #[must_use]
    pub fn wanted(&self, e: &Event, user_lvl: &[String]) -> bool {
        if !e.kind.implemented() {
            return false;
        }
        if self.noqtemp() && e.get("OLIB") == "QTEMP" {
            return false;
        }
        match e.kind.category() {
            None => self.objaud(),
            Some(c) => {
                self.has("*AUDLVL")
                    && (self.lvl.iter().any(|v| v == c)
                        || kind::expand(user_lvl).iter().any(|v| v == c))
            }
        }
    }
}

/// Whether an access by `op` is a change (`ZC`) rather than a read
/// (`ZR`): it needs anything beyond `*OBJOPR *READ *EXECUTE`.
#[must_use]
pub fn is_change(need: crate::auth::bits::Auth) -> bool {
    need.0 & !crate::auth::bits::Auth::USE.0 != 0
}

/// Whether an object's auditing value, with the user's, asks for an
/// access entry: `*ALL` audits reads and changes, `*CHANGE` changes;
/// `*USRPRF` defers to the user's `OBJAUD`.
#[must_use]
pub fn audits_access(object_value: &str, user_value: &str, change: bool) -> bool {
    let v = if object_value == "*USRPRF" {
        user_value
    } else {
        object_value
    };
    match v {
        "*ALL" => true,
        "*CHANGE" => change,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_select() {
        let e = Event::new(Kind::Af)
            .set("VIOL", "A")
            .object("PAYROLL", "APPLIB", "*FILE")
            .set("OPER", "file.open-input");
        let b = e.encode();
        assert_eq!(b.len(), layout::AF.size.unwrap());
        let back = Event::decode(Kind::Af, &b).unwrap();
        assert_eq!(back.get("ONAM"), "PAYROLL");
        assert_eq!(back.get("VIOL"), "A");
        let get = |n: &str| match n {
            "QAUDCTL" => "*AUDLVL *NOQTEMP".to_string(),
            "QAUDLVL" => "*AUTFAIL *SECURITY".to_string(),
            _ => String::new(),
        };
        let c = Control::from_sysvals(&get);
        assert!(c.on());
        assert!(c.wanted(&e, &[]));
        assert!(c.wanted(&Event::new(Kind::Ca), &[]));
        assert!(!c.wanted(&Event::new(Kind::Co), &[]));
        assert!(c.wanted(&Event::new(Kind::Co), &["*CREATE".into()]));
        let q = Event::new(Kind::Co).object("X", "QTEMP", "*DTAARA");
        assert!(!c.wanted(&q, &["*CREATE".into()]));
        assert!(!c.wanted(&Event::new(Kind::Zr), &[]));
        assert!(audits_access("*CHANGE", "", true));
        assert!(!audits_access("*CHANGE", "", false));
        assert!(audits_access("*USRPRF", "*ALL", false));
    }
}
