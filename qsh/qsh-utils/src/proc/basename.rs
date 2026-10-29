// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `basename string [suffix]` and `dirname string`, by the POSIX
//! algorithms.

use qsh::sys::UtilCtx;

use crate::common::io::Out;
use crate::common::usage;

/// The POSIX `basename`.
pub fn base(s: &str, suffix: Option<&str>) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.bytes().all(|b| b == b'/') {
        return "/".to_string();
    }
    let t = s.trim_end_matches('/');
    let mut b = t.rsplit('/').next().unwrap_or(t).to_string();
    if let Some(sfx) = suffix
        && !sfx.is_empty()
        && b != sfx
        && let Some(stripped) = b.strip_suffix(sfx)
    {
        b = stripped.to_string();
    }
    b
}

/// The POSIX `dirname`.
pub fn dir(s: &str) -> String {
    if s.bytes().all(|b| b == b'/') {
        return if s.is_empty() { ".".to_string() } else { "/".to_string() };
    }
    let t = s.trim_end_matches('/');
    match t.rfind('/') {
        None => ".".to_string(),
        Some(i) => {
            let d = t[..i].trim_end_matches('/');
            if d.is_empty() {
                "/".to_string()
            } else {
                d.to_string()
            }
        }
    }
}

/// Run `basename`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let args: Vec<&str> = cx.argv[1..].iter().map(String::as_str).collect();
    let args = if args.first() == Some(&"--") { &args[1..] } else { &args[..] };
    if args.is_empty() || args.len() > 2 {
        return usage(cx, "basename", "A string and an optional suffix are needed.");
    }
    let mut out = Out::stdout(cx);
    out.text(&base(args[0], args.get(1).copied()));
    out.text("\n");
    out.finish(cx, "basename", 0)
}

/// Run `dirname`.
pub fn run_dirname(cx: &mut UtilCtx<'_>) -> i32 {
    let args: Vec<&str> = cx.argv[1..].iter().map(String::as_str).collect();
    let args = if args.first() == Some(&"--") { &args[1..] } else { &args[..] };
    if args.len() != 1 {
        return usage(cx, "dirname", "One string is needed.");
    }
    let mut out = Out::stdout(cx);
    out.text(&dir(args[0]));
    out.text("\n");
    out.finish(cx, "dirname", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_examples() {
        assert_eq!(base("/usr/lib", None), "lib");
        assert_eq!(base("/usr/", None), "usr");
        assert_eq!(base("/", None), "/");
        assert_eq!(base("///", None), "/");
        assert_eq!(base("//usr//lib//", None), "lib");
        assert_eq!(base("a.c", Some(".c")), "a");
        assert_eq!(base(".c", Some(".c")), ".c");
        assert_eq!(base("", None), "");
        assert_eq!(dir("/usr/lib"), "/usr");
        assert_eq!(dir("/usr/"), "/");
        assert_eq!(dir("usr"), ".");
        assert_eq!(dir("/"), "/");
        assert_eq!(dir("//"), "/");
        assert_eq!(dir(""), ".");
        assert_eq!(dir("//usr//lib//"), "//usr");
    }
}
