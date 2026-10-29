// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file and directory utilities: `cat chmod chgrp chown cp df du
//! find ln ls mkdir mv rm rmdir touch`.

pub mod cat;
pub mod chmod;
pub mod chown;
pub mod cp;
pub mod df;
pub mod du;
pub mod find;
pub mod ln;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod rm;
pub mod rmdir;
pub mod touch;

/// The last component of a path (`/` for the root).
pub fn basename(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return if p.is_empty() { "" } else { "/" };
    }
    t.rsplit('/').next().unwrap_or(t)
}

/// Join a directory and a name.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}
