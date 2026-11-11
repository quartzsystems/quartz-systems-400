// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a scenario printed: `ok NAME` and its detail lines, `fail NAME:
//! why` or `skip NAME: why`.

use std::any::Any;

/// How one scenario ended.
pub enum Outcome {
    /// It held; the detail lines.
    Ok(Vec<String>),
    /// It did not; why.
    Fail(String),
    /// It was not run; why.
    Skip(String),
}

/// Prints `outcome` of scenario `name`.
pub fn print(name: &str, outcome: &Outcome) {
    match outcome {
        Outcome::Ok(lines) => {
            println!("ok {name}");
            for l in lines {
                println!("  {l}");
            }
        }
        Outcome::Fail(why) => println!("fail {name}: {}", why.replace('\n', " | ")),
        Outcome::Skip(why) => println!("skip {name}: {why}"),
    }
}

/// The text of a helper's panic.
pub fn panic_text(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "a helper panicked".into())
}

/// An NTSTATUS by name where the scenarios meet it, else in hex.
pub fn status(s: u32) -> String {
    use smb_server::ntstatus as nt;
    let name = match s {
        nt::STATUS_SUCCESS => "STATUS_SUCCESS",
        nt::STATUS_ACCESS_DENIED => "STATUS_ACCESS_DENIED",
        nt::STATUS_NOT_SUPPORTED => "STATUS_NOT_SUPPORTED",
        nt::STATUS_BAD_NETWORK_NAME => "STATUS_BAD_NETWORK_NAME",
        nt::STATUS_REQUEST_NOT_ACCEPTED => "STATUS_REQUEST_NOT_ACCEPTED",
        nt::STATUS_FILE_LOCK_CONFLICT => "STATUS_FILE_LOCK_CONFLICT",
        nt::STATUS_LOCK_NOT_GRANTED => "STATUS_LOCK_NOT_GRANTED",
        nt::STATUS_OBJECT_NAME_NOT_FOUND => "STATUS_OBJECT_NAME_NOT_FOUND",
        nt::STATUS_LOGON_FAILURE => "STATUS_LOGON_FAILURE",
        _ => return format!("0x{s:08X}"),
    };
    name.into()
}
