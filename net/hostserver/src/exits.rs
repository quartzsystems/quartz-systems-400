// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Exit points: before a server acts on a request it asks the exit
//! programs registered on the server's point (`regfac::call_exit` in a
//! job); a refusal answers the request as not authorized.

use regfac::ExitRequest;
use regfac::formats::host::Who;

/// Who answers exit point questions.
pub trait Exits {
    /// Whether the exit programs of `point` allow `req`.
    fn allow(&mut self, point: &str, req: &ExitRequest) -> bool;
}

/// No exit programs: everything is allowed.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoExits;

impl Exits for NoExits {
    fn allow(&mut self, _point: &str, _req: &ExitRequest) -> bool {
        true
    }
}

/// The caller part of every host server format.
pub fn who(user: &str, function: i32) -> Who {
    Who {
        user: user.to_string(),
        function,
    }
}
