// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The administration instance's pages, Quartz-written: the one page
//! saying web administration is not here yet, and the short error
//! pages.

/// What the page says about the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteInfo {
    /// The system name (`QSYSNAME` / network attribute `SYSNAME`).
    pub system: String,
    /// The release (`V1R0M0`).
    pub release: String,
}

impl SiteInfo {
    /// The running release on `system`.
    pub fn new(system: &str) -> Self {
        Self {
            system: system.trim().to_string(),
            release: release::RELEASE.to_string(),
        }
    }
}

/// Text safe inside HTML.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn document(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>\n\
         body {{ font-family: system-ui, sans-serif; margin: 3em auto; max-width: 40em; padding: 0 1em; color: #222; }}\n\
         h1 {{ font-size: 1.4em; }}\n\
         dl {{ display: grid; grid-template-columns: max-content auto; gap: .3em 1em; }}\n\
         dt {{ font-weight: bold; }}\n\
         </style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(title)
    )
}

/// The administration page.
pub fn admin(info: &SiteInfo) -> String {
    let system = escape(&info.system);
    let release = escape(&info.release);
    document(
        &format!("{} web administration", info.system),
        &format!(
            "<h1>Web administration is not yet available</h1>\n\
             <p>The web administration interface of QS/400 is still to come. \
             Until it is here, manage this system from a 5250 session: \
             CFGTCP, WRKACTJOB and the other commands do everything this page will.</p>\n\
             <dl>\n<dt>System</dt><dd>{system}</dd>\n<dt>Release</dt><dd>QS/400 {release}</dd>\n</dl>\n"
        ),
    )
}

/// An error page: the status line's code and reason, and a sentence.
pub fn error(code: u16, reason: &str, says: &str) -> String {
    document(
        &format!("{code} {reason}"),
        &format!("<h1>{code} {}</h1>\n<p>{}</p>\n", escape(reason), escape(says)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_names_the_system_and_release() {
        let p = admin(&SiteInfo::new("QS400<1>"));
        assert!(p.contains("not yet available"));
        assert!(p.contains("QS400&lt;1&gt;"));
        assert!(p.contains("QS/400 V1R0M0"));
        assert!(error(404, "Not Found", "No such page.").contains("<h1>404 Not Found</h1>"));
    }
}
