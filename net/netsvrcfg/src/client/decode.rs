// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list records read back: `ZLSL0101` shares and the `ZLSL0201`
//! attributes, the 7.5 layouts [`crate::fmt::zlsl`] writes.

use crate::fmt::text::{bin4, ch, text_at};
use crate::fmt::zlsl::EXTENSION_SIZE;
use crate::model::attrs::{Attributes, Settings};
use crate::model::share::{FileShare, Kind, Permission, PrintShare, Qualified, Share, SpoolType};

fn size(b: &[u8], off: usize) -> usize {
    usize::try_from(bin4(b, off)).unwrap_or(0)
}

/// A path's bytes in `ccsid` as text.
fn path_text(b: &[u8], ccsid: u16) -> String {
    if ccsid != 0
        && ccsid != 37
        && let Ok(s) = ccsid::conv::to_string(ccsid, b)
    {
        return s;
    }
    ccsid::from_ebcdic(b)
}

fn qualified(b: &[u8], off: usize) -> Qualified {
    Qualified::new(&text_at(b, off, 10), &text_at(b, off + 10, 10))
}

/// One `ZLSL0101` record starting at `at` of the receiver `all`
/// (offsets count from the receiver's start).
fn share_at(all: &[u8], at: usize, ccsid: u16) -> Option<Share> {
    let r = all.get(at..)?;
    let name = text_at(r, 4, 12);
    let text = text_at(r, 114, 50);
    let kind = if bin4(r, 16) == 1 {
        let printer_file = qualified(r, 164);
        Kind::Print(PrintShare {
            outq: Qualified::new(&text_at(r, 44, 10), &text_at(r, 54, 10)),
            driver: text_at(r, 64, 50),
            spool_type: SpoolType::from_code(bin4(r, 32)).unwrap_or(SpoolType::Auto),
            printer_file: if printer_file.name.is_empty() {
                Qualified::default()
            } else {
                printer_file
            },
            publish: ch(r, 197) == '1',
        })
    } else {
        let (po, pl) = (size(r, 36), size(r, 40));
        let path = all
            .get(po..po + pl)
            .map_or_else(String::new, |b| path_text(b, ccsid));
        let (eo, en) = (size(r, 188), size(r, 192));
        let extensions = (0..en)
            .filter_map(|i| {
                let e = all.get(eo + i * EXTENSION_SIZE..eo + (i + 1) * EXTENSION_SIZE)?;
                let l = size(e, 0).min(46);
                Some(text_at(e, 4, l))
            })
            .collect();
        Kind::File(FileShare {
            path,
            permission: Permission::from_code(bin4(r, 20)).unwrap_or(Permission::ReadOnly),
            max_users: bin4(r, 24),
            text_ccsid: bin4(r, 184),
            text_conversion: ch(r, 196) == '1',
            extensions,
        })
    };
    Some(Share {
        name,
        text,
        encryption_required: ch(r, 198) == '1',
        autl: text_at(r, 199, 10),
        kind,
        unknown: Vec::new(),
    })
}

/// The `n` `ZLSL0101` records of receiver `data`.
#[must_use]
pub fn shares(data: &[u8], n: usize, ccsid: u16) -> Vec<Share> {
    let mut out = Vec::with_capacity(n);
    let mut at = 0;
    for _ in 0..n {
        let len = size(data, at);
        let Some(s) = share_at(data, at, ccsid) else {
            break;
        };
        out.push(s);
        if len == 0 {
            break;
        }
        at += len;
    }
    out
}

fn side(b: &[u8], pending: bool) -> Settings {
    let i = |off: usize| bin4(b, off + if pending { 4 } else { 0 });
    let c = |off: usize, n: usize| text_at(b, off + if pending { n } else { 0 }, n);
    let f = |off: usize| ch(b, off + usize::from(pending)) == '1';
    let mut s = Settings::new("");
    s.code_page = i(0);
    s.idle_timeout = i(8);
    s.oplock_timeout = i(16);
    s.browse_interval = i(24);
    s.wins_proxy = i(32) == 1;
    s.guest = c(48, 10);
    s.server_name = c(68, 15);
    s.domain = c(98, 15);
    s.text = c(128, 50);
    s.wins_primary = c(228, 15);
    s.wins_secondary = c(258, 15);
    s.scope = c(288, 224);
    s.system_name_access = f(736);
    s.auth_method = ch(b, 738 + usize::from(pending))
        .to_digit(10)
        .and_then(|d| u8::try_from(d).ok())
        .unwrap_or(0);
    s.server_role = i(740);
    s.message_auth = i(748);
    s.min_message_severity = i(756);
    s.lanman_auth = i(764);
    s.encryption = i(772);
    s.server_autl = c(780, 10);
    s
}

/// The `ZLSL0201` record: the attributes IBM's format has (QS/400's own
/// keep their defaults).
#[must_use]
pub fn attributes(record: &[u8]) -> Attributes {
    Attributes {
        current: side(record, false),
        pending: side(record, true),
    }
}
