// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The text forms of the IFS displays, for a job without a display
//! (the service console, batch, `OUTPUT(*PRINT)`): `WRKLNK` and
//! `DSPLNK`'s list, an object's attributes, `DSPAUT`/`WRKAUT`'s rows,
//! and `DSPF`'s file.

use std::fmt::Write as _;

use ifs::client::{IfsClient, read_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_RDONLY, O_TEXTDATA};
use ifs::Errno;
use sec::auth::bits::Auth;

use crate::display::{LABELS, attributes, links, shown_name};
use crate::msg::{Msg, Reply};

/// `WRKLNK`/`DSPLNK` as text: the directory, then one line per link.
pub fn links_text(c: &dyn IfsClient, obj: &str, objtype: &str) -> Reply {
    let l = match links(c, obj, objtype) {
        Ok(l) => l,
        Err(e) => return Reply::errno(e, obj),
    };
    if l.links.is_empty() {
        return Reply::fail(Msg::path("CPFA0B2", obj));
    }
    let mut t = String::new();
    let _ = writeln!(t, " Directory  . . . . :   {}", l.dir);
    let _ = writeln!(t, " Object link            Type     Attribute    Text");
    for k in &l.links {
        let _ = writeln!(
            t,
            " {:<22} {:<8} {:<12} {}",
            shown_name(&k.name),
            k.objtype,
            k.attr,
            k.text
        );
    }
    Reply::text(t)
}

/// An object's attributes as text, one labelled line each.
pub fn attributes_text(c: &dyn IfsClient, path: &str, have_audit: bool) -> Reply {
    let v = match attributes(c, path, have_audit) {
        Ok(v) => v,
        Err(e) => return Reply::errno(e, path),
    };
    let mut t = String::new();
    for (k, val) in v {
        let label = LABELS.iter().find(|(n, _)| *n == k).map_or(k, |(_, l)| *l);
        let lead = format!(" {label} {}", ". ".repeat(20));
        let lead: String = lead.chars().take(40).collect();
        let _ = writeln!(t, "{lead}:   {val}");
    }
    Reply::text(t)
}

/// The four object authority columns as `X` marks.
fn marks(a: Auth) -> String {
    let bit = |n: &str| sec::auth::bits::NAMES.iter().find(|(k, _)| *k == n).map_or(0, |(_, b)| *b);
    let x = |n: &str| if a.0 & bit(n) != 0 { "X" } else { " " };
    format!(
        "{:^6}{:^5}{:^7}{:^5}",
        x("*OBJEXIST"),
        x("*OBJMGT"),
        x("*OBJALTER"),
        x("*OBJREF")
    )
}

/// `DSPAUT`/`WRKAUT` as text.
pub fn authority_text(c: &dyn IfsClient, path: &str) -> Reply {
    let a = match c.dspaut(path) {
        Ok(a) => a,
        Err(e) => return Reply::errno(e, path),
    };
    let mut t = String::new();
    let _ = writeln!(t, " Object . . . . . . . . . . . . :   {}", a.path);
    let _ = writeln!(t, " Type . . . . . . . . . . . . . :   {}", a.objtype.trim_start_matches('*'));
    let _ = writeln!(t, " Owner  . . . . . . . . . . . . :   {}", a.owner);
    let _ = writeln!(
        t,
        " Primary group  . . . . . . . . :   {}",
        a.group.as_deref().unwrap_or("*NONE")
    );
    let _ = writeln!(
        t,
        " Authorization list . . . . . . :   {}",
        a.autl.as_deref().unwrap_or("*NONE")
    );
    let _ = writeln!(t, "                    Data     --Object Authorities--");
    let _ = writeln!(t, " User        Authority  Exist  Mgt  Alter  Ref");
    for e in &a.entries {
        let _ = writeln!(t, " {:<11} {:<10} {}", e.who, e.data, marks(e.objects));
    }
    Reply::text(t)
}

/// `DSPF` as text: the file in the job's characters, one line per
/// line. A file that does not convert (a binary one) shows as its
/// bytes in hexadecimal, 32 to a line.
pub fn file_text(c: &dyn IfsClient, path: &str) -> Reply {
    match c.stat(path, true) {
        Ok(s) if s.objtype == "*DIR" || s.objtype == "*LIB" || s.objtype == "*FILE" => {
            return Reply::errno(Errno::EISDIR, path);
        }
        Ok(_) => {}
        Err(e) => return Reply::errno(e, path),
    }
    let text = OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208);
    let (fd, converted) = match c.open(path, text) {
        Ok(fd) => (fd, true),
        Err(Errno::ECONVERT) => match c.open(path, OpenArgs::new(O_RDONLY)) {
            Ok(fd) => (fd, false),
            Err(e) => return Reply::errno(e, path),
        },
        Err(e) => return Reply::errno(e, path),
    };
    let data = read_all(c, fd);
    let _ = c.close(fd);
    let data = match data {
        Ok(d) => d,
        Err(e) => return Reply::errno(e, path),
    };
    let mut t = String::new();
    if converted {
        let s = String::from_utf8_lossy(&data);
        for line in s.split_inclusive('\n') {
            let l = line.trim_end_matches(['\n', '\r']).replace('\t', " ");
            let _ = writeln!(t, "{l}");
        }
    } else {
        for chunk in data.chunks(32) {
            for b in chunk {
                let _ = write!(t, "{b:02X}");
            }
            t.push('\n');
        }
    }
    Reply::text(t)
}
