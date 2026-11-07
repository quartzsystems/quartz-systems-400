// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client pieces Access for Web's Download page offers
//! (`net/accessweb`): the storage service keeps them in [`DIR`], with
//! the `INDEX.txt` that describes them (section, file, label, note,
//! tab-separated). Other files put there show under Other files.

/// Their directory in the IFS, and the directories above it.
pub const DIRS: [&str; 3] = ["/QQS/ProdData/Access", "/QQS/ProdData/Access/Web", DIR];
/// The download directory.
pub const DIR: &str = "/QQS/ProdData/Access/Web/download";

/// The index.
const INDEX: &str = "# Access for Web downloads: section, file, label, note (tab-separated).\n\
Terminal\tqs400-9x16.txt\tQS/400 terminal font (9x16 bitmap source of the 5250 display and the console)\t\n";

/// The files: name and bytes.
pub const FILES: &[(&str, &[u8])] = &[
    ("INDEX.txt", INDEX.as_bytes()),
    (
        "qs400-9x16.txt",
        include_bytes!("../../console/font/qs400-9x16.txt"),
    ),
];
