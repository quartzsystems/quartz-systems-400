// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Create certificate (not built yet).

use qsweb::http::Response;

use super::Rq;

/// `/certs/create/step/<n>`.
pub fn step(rq: &Rq<'_>, _n: &str) -> Response {
    rq.not_found("create certificate page")
}

/// `/certs/create/result`.
pub fn result(rq: &Rq<'_>) -> Response {
    rq.not_found("create certificate page")
}
