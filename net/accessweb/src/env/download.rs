// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The download page's files under `/QQS/ProdData/Access/Web/download`
//! (handoff step 21), read as the signed-on user.

use qsweb::msg::Message;

/// Where the files are.
pub const DIR: &str = "/QQS/ProdData/Access/Web/download";

/// One file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DlFile {
    /// Its section (`Printer drivers`, `Terminal`, ...).
    pub section: String,
    /// Its name in the directory.
    pub file: String,
    /// What the link says.
    pub label: String,
    /// Bytes.
    pub size: u64,
    /// A note after the link; empty for none.
    pub note: String,
}

/// What the download page needs.
pub trait Download: Send + Sync {
    /// The files, in the order the page shows them.
    fn files(&self, user: &str) -> Vec<DlFile>;
    /// File `file`'s bytes, read as `user`.
    fn read(&self, user: &str, file: &str) -> Result<Vec<u8>, Message>;
}
