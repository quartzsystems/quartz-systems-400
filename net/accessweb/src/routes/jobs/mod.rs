// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Jobs: `/jobs`, `/jobs/server`. Not built yet: every route answers the honest Not
//! available page (`XWA0099`) until the area's pages land here.

use qsweb::http::Response;

use super::Rq;

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, _segs: &[&str]) -> Response {
    // BLOCKED(rel): Jobs's pages are another agent's step; refused with XWA0099 until then.
    super::shared::not_available(rq, "Jobs", "Jobs")
}
