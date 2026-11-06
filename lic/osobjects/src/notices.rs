// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The third-party notices (`shipped/NOTICES.txt`, `build notices`
//! writes it from `Cargo.lock`): the storage service keeps them at
//! [`PATH`] in the IFS, and the install media at their root.

/// The notices.
pub const TEXT: &str = include_str!("../shipped/NOTICES.txt");
/// Their directory in the IFS.
pub const DIR: &str = "/QQS/ProdData/Notices";
/// The file in the IFS.
pub const PATH: &str = "/QQS/ProdData/Notices/NOTICES.txt";
