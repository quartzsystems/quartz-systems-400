// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Import certificate (not built yet).

use qsweb::http::Response;

use super::Rq;

/// `/certs/import`.
pub fn page(rq: &Rq<'_>) -> Response {
    rq.not_found("import page")
}
