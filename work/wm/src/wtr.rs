// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a writer job is told to do next.

use ipc::wire_enum;

use crate::obj::splf::SplfAttrs;

/// The writer's next step.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // one at a time, over the wire
pub enum WtrStep {
    /// End: `ENDWTR`, or `AUTOEND(*YES)` with the queue empty.
    End,
    /// Nothing to print now (held, or the queue is empty or held).
    Wait,
    /// Print this file.
    File {
        /// Its attributes.
        attrs: SplfAttrs,
        /// Its SCS.
        data: Vec<u8>,
    },
}

wire_enum!(WtrStep {
    0 => End,
    1 => Wait,
    2 => File { attrs, data },
});
