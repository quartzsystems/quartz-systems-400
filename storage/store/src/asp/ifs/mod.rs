// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS in the ASP: the volume's extents, held by the allocator and
//! listed in the map. See `docs/STORE_FORMAT.md` §12.

pub mod map;

pub use map::IfsExtent;
