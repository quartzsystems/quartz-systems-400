// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a family of web products supplies to every page and response:
//! its name, vendor and release, the `Server` header's token, where the
//! stylesheet and the navigation script are served, the session
//! cookie's name, and the sentences the shared pages say about its
//! policies. Each product names its family in [`crate::frame::Product`].

use std::fmt::{self, Display};

/// A family of web products.
#[derive(Clone, Copy)]
pub struct Brand {
    /// The family's name: the shared pages' wordmark and title suffix,
    /// the help window's, the footer's and the sign-on notice's.
    pub name: &'static str,
    /// The vendor, in the footer and the sign-on notice.
    pub vendor: &'static str,
    /// The release, in the footer, the sign-on notice and `Server`.
    pub release: &'static (dyn Display + Sync),
    /// The `Server` header's product token, before `/` and the release.
    pub server: &'static str,
    /// Where the stylesheet ([`crate::CSS`]) is served.
    pub css_path: &'static str,
    /// Where the navigation script ([`crate::NAV_JS`]) is served.
    pub nav_path: &'static str,
    /// The session cookie's name.
    pub cookie: &'static str,
    /// The sign-on page's line under the copyright; empty, the
    /// copyright line stands alone.
    pub signon_notice: &'static str,
    /// Where a product's sign-on notice ([`crate::frame::signon`]'s
    /// `notice`) goes: false above the form, true as the sign-on
    /// table's first row.
    pub notice_in_form: bool,
    /// The most the sign-on page's User field takes.
    pub user_max: u32,
    /// The Not authorized page's closing paragraph, HTML: whom to ask,
    /// where the attempt was recorded.
    pub notauth_note: &'static str,
    /// Whether the error page offers `Job log`.
    pub job_log: bool,
}

impl Brand {
    /// The footer line: name, release, vendor.
    pub fn footer(&self) -> String {
        format!("{} {} \u{b7} {}", self.name, self.release, self.vendor)
    }

    /// The `Server` header's value: token `/` release.
    pub fn server_header(&self) -> String {
        format!("{}/{}", self.server, self.release)
    }

    /// The release as text.
    pub fn release(&self) -> String {
        self.release.to_string()
    }
}

impl fmt::Debug for Brand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Brand")
            .field("name", &self.name)
            .field("release", &self.release.to_string())
            .field("server", &self.server)
            .field("cookie", &self.cookie)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::Brand;

    /// A brand for this crate's tests.
    pub static TEST: Brand = Brand {
        name: "Demo",
        vendor: "Example",
        release: &"1.0",
        server: "Demo",
        css_path: "/s/site.css",
        nav_path: "/s/nav.js",
        cookie: "DEMOSID",
        signon_notice: "Authorized users only.",
        notice_in_form: false,
        user_max: 10,
        notauth_note: "Ask the administrator.",
        job_log: false,
    };

    #[test]
    fn lines() {
        assert_eq!(TEST.footer(), "Demo 1.0 \u{b7} Example");
        assert_eq!(TEST.server_header(), "Demo/1.0");
    }
}
