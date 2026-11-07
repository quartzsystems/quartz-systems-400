// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The preference store: per user, the settings that differ from the
//! defaults (`QUSRSYS/QZXWPRF` on the system). What each one means is
//! [`crate::prefs`]'s.

use qsweb::msg::Message;

/// Where preferences are kept.
pub trait PrefStore: Send + Sync {
    /// `user`'s stored values: key and value.
    fn get(&self, user: &str) -> Vec<(String, String)>;
    /// Replace `user`'s stored values with `values`.
    fn put(&self, user: &str, values: &[(String, String)]) -> Result<(), Message>;
    /// The schemas Preferences offers `user` as the default schema.
    fn schemas(&self, user: &str) -> Vec<String>;
}
