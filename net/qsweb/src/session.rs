// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions: an opaque cookie naming a server-side entry with the
//! signed-on user, the CSRF token, the next page's message and a
//! product's own values (wizard state). Entries idle longer than the
//! timeout are gone.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::msg::Message;

/// The session cookie's name.
pub const COOKIE: &str = "QS400SID";
/// The hidden form field carrying the CSRF token.
pub const CSRF_FIELD: &str = "csrf";

/// One signed-on user's session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The cookie value.
    pub id: String,
    /// The user profile.
    pub user: String,
    /// The token every POST must carry.
    pub csrf: String,
    /// The message the next page shows once.
    pub flash: Option<Message>,
    /// The product's values.
    pub data: BTreeMap<String, String>,
}

impl Session {
    /// A product value, or "".
    pub fn get(&self, key: &str) -> &str {
        self.data.get(key).map_or("", String::as_str)
    }
}

struct Entry {
    s: Session,
    last: Instant,
}

/// The table of sessions.
pub struct Sessions {
    map: Mutex<HashMap<String, Entry>>,
    idle: Mutex<Duration>,
}

impl Sessions {
    /// A table whose sessions end after `idle` without a request.
    pub fn new(idle: Duration) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            idle: Mutex::new(idle),
        }
    }

    /// Change the idle timeout (Global settings).
    pub fn set_idle(&self, idle: Duration) {
        *self.idle.lock().expect("sessions") = idle;
    }

    fn idle(&self) -> Duration {
        *self.idle.lock().expect("sessions")
    }

    /// Start a session for `user`.
    pub fn create(&self, user: &str) -> Session {
        let s = Session {
            id: token(),
            user: user.to_string(),
            csrf: token(),
            flash: None,
            data: BTreeMap::new(),
        };
        let mut map = self.map.lock().expect("sessions");
        let idle = self.idle();
        map.retain(|_, e| e.last.elapsed() < idle);
        map.insert(s.id.clone(), Entry { s: s.clone(), last: Instant::now() });
        s
    }

    /// The live session `id`, touched.
    pub fn get(&self, id: &str) -> Option<Session> {
        let idle = self.idle();
        let mut map = self.map.lock().expect("sessions");
        match map.get_mut(id) {
            Some(e) if e.last.elapsed() < idle => {
                e.last = Instant::now();
                Some(e.s.clone())
            }
            Some(_) => {
                map.remove(id);
                None
            }
            None => None,
        }
    }

    /// Change session `id`; false when it is gone.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Session)) -> bool {
        let mut map = self.map.lock().expect("sessions");
        match map.get_mut(id) {
            Some(e) => {
                f(&mut e.s);
                true
            }
            None => false,
        }
    }

    /// Take the message waiting for session `id`.
    pub fn take_flash(&self, id: &str) -> Option<Message> {
        let mut out = None;
        self.update(id, |s| out = s.flash.take());
        out
    }

    /// Leave a message for the next page of session `id`.
    pub fn flash(&self, id: &str, m: Message) {
        self.update(id, |s| s.flash = Some(m));
    }

    /// End session `id` (Sign off).
    pub fn remove(&self, id: &str) {
        self.map.lock().expect("sessions").remove(id);
    }

    /// Sessions alive now.
    pub fn count(&self) -> usize {
        let idle = self.idle();
        self.map.lock().expect("sessions").values().filter(|e| e.last.elapsed() < idle).count()
    }
}

/// The session value naming the open navigation groups.
const NAV_OPEN: &str = "nav.open";
/// The session value naming the group of the last page shown.
const NAV_LAST: &str = "nav.last";

/// The navigation groups open in `s`.
pub fn nav_open(s: &Session) -> Vec<String> {
    s.get(NAV_OPEN).split('|').filter(|l| !l.is_empty()).map(ToString::to_string).collect()
}

fn set_open(s: &mut Session, open: &[String]) {
    s.data.insert(NAV_OPEN.into(), open.join("|"));
}

/// A page of `group` is shown: arriving from another group opens it.
/// Gives the groups open now.
pub fn nav_visit(sessions: &Sessions, id: &str, group: &str) -> Vec<String> {
    let mut out = Vec::new();
    sessions.update(id, |s| {
        let mut open = nav_open(s);
        if s.get(NAV_LAST) != group {
            if !open.iter().any(|l| l == group) {
                open.push(group.to_string());
            }
            set_open(s, &open);
            s.data.insert(NAV_LAST.into(), group.to_string());
        }
        out = open;
    });
    out
}

/// `[+]` or `[-]` of the tree: `open=` or `close=` a group.
pub fn nav_toggle(sessions: &Sessions, id: &str, open: Option<&str>, close: Option<&str>) {
    sessions.update(id, |s| {
        let mut set = nav_open(s);
        if let Some(g) = open
            && !set.iter().any(|l| l == g)
        {
            set.push(g.to_string());
        }
        if let Some(g) = close {
            set.retain(|l| l != g);
        }
        set_open(s, &set);
    });
}

/// 32 random bytes as hex.
pub fn token() -> String {
    let mut b = [0u8; 32];
    graviola::random::fill(&mut b).expect("system random source");
    hex::encode(b)
}

/// Whether `sent` is the session's CSRF token (constant time).
pub fn csrf_ok(s: &Session, sent: &str) -> bool {
    let a = s.csrf.as_bytes();
    let b = sent.as_bytes();
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The `Set-Cookie` value for session `id` under `path`.
pub fn set_cookie(id: &str, path: &str, secure: bool) -> String {
    format!(
        "{COOKIE}={id}; Path={path}; HttpOnly; SameSite=Strict{}",
        if secure { "; Secure" } else { "" }
    )
}

/// The `Set-Cookie` value that removes the cookie.
pub fn clear_cookie(path: &str) -> String {
    format!("{COOKIE}=; Path={path}; HttpOnly; SameSite=Strict; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::Kind;

    #[test]
    fn nav_groups() {
        let t = Sessions::new(Duration::from_secs(60));
        let id = t.create("QSECOFR").id;
        assert_eq!(nav_visit(&t, &id, "Manage"), ["Manage"]);
        nav_toggle(&t, &id, Some("Create"), None);
        assert_eq!(nav_visit(&t, &id, "Manage"), ["Manage", "Create"]);
        // Closing the page's own group sticks while the user stays there.
        nav_toggle(&t, &id, None, Some("Manage"));
        assert_eq!(nav_visit(&t, &id, "Manage"), ["Create"]);
        // Arriving from another group opens it again.
        assert_eq!(nav_visit(&t, &id, "Create"), ["Create"]);
        assert_eq!(nav_visit(&t, &id, "Manage"), ["Create", "Manage"]);
    }

    #[test]
    fn lifecycle() {
        let t = Sessions::new(Duration::from_secs(60));
        let s = t.create("QSECOFR");
        assert_eq!(s.id.len(), 64);
        assert_ne!(s.id, s.csrf);
        assert!(csrf_ok(&s, &s.csrf.clone()));
        assert!(!csrf_ok(&s, "x"));
        t.flash(&s.id, Message::new(Kind::Done, "HTP8001", "started"));
        assert!(t.update(&s.id, |s| {
            s.data.insert("wiz.name".into(), "WEBSTORE".into());
        }));
        assert_eq!(t.get(&s.id).unwrap().get("wiz.name"), "WEBSTORE");
        assert_eq!(t.take_flash(&s.id).unwrap().id, "HTP8001");
        assert!(t.take_flash(&s.id).is_none());
        t.remove(&s.id);
        assert!(t.get(&s.id).is_none());
        let t = Sessions::new(Duration::ZERO);
        let s = t.create("A");
        assert!(t.get(&s.id).is_none());
        assert!(set_cookie("x", "/qs400/", true).ends_with("; Secure"));
    }
}
