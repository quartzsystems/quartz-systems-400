// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Control panel function 21 from the QSI server (`docs/QSID.md`
//! section 12): the server job leaves [`REQUEST`] and sends `SIGUSR1`,
//! as a power down's request does; `slic` takes it, shows the service
//! tools on the console as for the partition service channel's function
//! 21, and writes [`ANSWER`]: `ok`, or a message identifier and its
//! text.

use std::path::Path;

/// The request: the line `21`, then the console's user.
pub const REQUEST: &str = "/run/qs400/qsipanel";
/// The answer.
pub const ANSWER: &str = "/run/qs400/qsipanel.answer";

/// Whether a request is there: the signal is the server's, not a power
/// down's. The request is taken.
pub fn take() -> Option<String> {
    if !Path::new(REQUEST).exists() {
        return None;
    }
    let text = std::fs::read_to_string(REQUEST).unwrap_or_default();
    let _ = std::fs::remove_file(REQUEST);
    Some(text)
}

/// Answer the request: `None` taken, else the refusal.
pub fn answer(refusal: Option<(&str, &str)>) {
    let text = match refusal {
        None => "ok\n".to_string(),
        Some((id, t)) => format!("{id} {t}\n"),
    };
    let tmp = format!("{ANSWER}.new");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, ANSWER);
    }
}

/// While the service tools show already: a request is refused at once.
/// Whether there was one.
pub fn refuse_busy() -> bool {
    if take().is_none() {
        return false;
    }
    answer(Some((
        qsipart::service::msgid::DST_BUSY,
        guestagent::qsid::text::DST_ACTIVE,
    )));
    true
}
