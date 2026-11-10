// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QZLS*` APIs over their parameters' bytes ([`parms`]): the job's
//! native gathers each parameter, the storage service calls [`call`]
//! with a [`Host`], and the answer (or the error and its data) goes
//! back for the receiver and the error code.

pub mod error;
pub mod host;
pub mod lists;
pub mod parms;
pub mod server;
pub mod shares;

pub use error::ApiError;
pub use host::{Caller, Enabled, GuestCheck, Host, MemHost, NoServer, ServerControl};
pub use parms::Parms;

/// Every API by name.
pub const APIS: &[&str] = &[
    "QZLSADFS", "QZLSADPS", "QZLSCHFS", "QZLSCHPS", "QZLSCHSG", "QZLSCHSI", "QZLSCHSN", "QZLSENDS",
    "QZLSENSS", "QZLSLSTI", "QZLSOLST", "QZLSRMS", "QZLSSTRS",
];

/// Run API `api` with parameters `parms`.
pub fn call(h: &mut dyn Host, api: &str, parms: &[Vec<u8>]) -> Result<Vec<u8>, ApiError> {
    let p = Parms(parms);
    match api {
        "QZLSADFS" => shares::add_file(h, &p),
        "QZLSCHFS" => shares::change_file(h, &p),
        "QZLSADPS" => shares::add_print(h, &p),
        "QZLSCHPS" => shares::change_print(h, &p),
        "QZLSRMS" => shares::remove(h, &p),
        "QZLSCHSN" => server::change_name(h, &p),
        "QZLSCHSG" => server::change_guest(h, &p),
        "QZLSCHSI" => server::change_info(h, &p),
        "QZLSSTRS" => server::start(h, &p),
        "QZLSENDS" => server::end(h, &p),
        "QZLSENSS" => server::end_sessions(h, &p),
        "QZLSLSTI" => lists::lsti(h, &p),
        "QZLSOLST" => lists::olst(h, &p),
        _ => Err(ApiError::new(error::msg::INTERNAL)),
    }
}
