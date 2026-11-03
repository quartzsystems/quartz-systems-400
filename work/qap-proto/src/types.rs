// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Types several services share (docs/QAP.md, "Common types").

use ipc::wire_struct;

/// A job: name, user, six-digit number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct JobId {
    /// The job name.
    pub name: String,
    /// Its user.
    pub user: String,
    /// Its number, six digits.
    pub number: String,
}

wire_struct!(JobId { name, user, number });

/// An object by library and name; the library may be `*LIBL` or
/// `*CURLIB` where a library list search applies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ObjectName {
    /// The library.
    pub library: String,
    /// The object.
    pub name: String,
}

wire_struct!(ObjectName { library, name });
