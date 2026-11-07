// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's home directory `/home/CWELLMAN`, as
//! `web/files.html` lists it.

use crate::env::files::Files;

/// The home directory.
pub const HOME: &str = "/home/CWELLMAN";

/// An entry: name, type, size, modified, owner, authority, CCSID.
pub type Entry = [&'static str; 7];

/// The home directory's entries.
pub const ENTRIES: &[Entry] = &[
    ["certs", "Directory", "", "10/20/26 11:02", "CWELLMAN", "rwx------", ""],
    ["scripts", "Directory", "", "10/28/26 17:40", "CWELLMAN", "rwxr-x---", ""],
    ["upload", "Directory", "", "10/29/26 13:30", "CWELLMAN", "rwxrwx---", ""],
    [".profile", "File", "412", "09/01/26 08:00", "CWELLMAN", "rw-------", "1208"],
    ["assets_br1.csv", "File", "2,118", "10/29/26 13:30", "CWELLMAN", "rw-r-----", "1208"],
    ["inventory_2026-10.pdf", "File", "240,112", "10/28/26 18:02", "CWELLMAN", "rw-r--r--", "65535"],
    ["invload.clle", "File", "1,844", "10/29/26 13:05", "CWELLMAN", "rw-r-----", "1208"],
    ["notes.txt", "File", "9,310", "10/27/26 21:15", "CWELLMAN", "rw-------", "1208"],
    ["reports", "Link -> /QS400/UserData/Shares/docs/reports", "", "10/01/26 09:00", "CWELLMAN", "rwxrwxrwx", ""],
    ["old-link", "Link -> /home/CWELLMAN/gone (broken)", "", "09/01/26 08:00", "CWELLMAN", "rwxrwxrwx", ""],
    ["webstore.p12", "File", "3,402", "10/20/26 11:02", "CWELLMAN", "rw-------", "65535"],

];

/// The sample file system.
#[derive(Debug, Default)]
pub struct FixtureFiles;

impl Files for FixtureFiles {}
