// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: `/jobs`, `/job/<number>/<user>/<name>/...`, `/jobq`, `/sbs`, `/outq`, `/splf`, `/writers`, `/jobscde`.

use qsweb::http::Response;

use super::Rq;

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    rq.not_found(&segs.join("/"))
}
