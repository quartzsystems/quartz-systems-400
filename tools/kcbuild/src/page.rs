// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The page frame: header band, navigation column, content, footer.

use std::fmt::Write as _;

use crate::md::esc;

/// One page: where it sits (`depth` directories below the root, for
/// relative links), its breadcrumb, title and body.
pub struct Page<'a> {
    pub depth: usize,
    pub crumb: &'a [(&'a str, &'a str)],
    pub title: &'a str,
    pub desc: &'a str,
    pub body: &'a str,
}

const NAV: &[(&str, &str)] = &[
    ("Home", "index.html"),
    ("Command finder", "commands/index.html"),
    ("Message finder", "messages/index.html"),
    ("Documents", "docs/index.html"),
    ("Conformance matrix", "conformance.html"),
];

impl Page<'_> {
    /// The whole document.
    pub fn render(&self) -> String {
        let up = "../".repeat(self.depth);
        let mut nav = String::new();
        for (name, href) in NAV {
            let _ = writeln!(
                nav,
                "<div class=\"navitem\"><a href=\"{up}{href}\">{name}</a></div>\n"
            );
        }
        let mut crumb = String::from("Knowledge Center");
        for (t, href) in self.crumb {
            if href.is_empty() {
                let _ = write!(crumb, " &gt; {}", esc(t));
            } else {
                let _ = write!(crumb, " &gt; <a href=\"{up}{href}\">{}</a>", esc(t));
            }
        }
        let desc = if self.desc.is_empty() {
            String::new()
        } else {
            format!("<p class=\"desc\">{}</p>\n", esc(self.desc))
        };
        format!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \
             \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n\
             <html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head>\n\
             <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\" />\n\
             <title>{title} - Knowledge Center for QS/400</title>\n\
             <link rel=\"stylesheet\" type=\"text/css\" href=\"{up}qs400.css\" />\n</head>\n<body>\n\
             <table class=\"frame\" cellspacing=\"0\" cellpadding=\"0\">\n\
             <tr><td class=\"hdr\" colspan=\"2\"><table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr>\
             <td class=\"wordmark\">Knowledge Center for QS/400</td>\
             <td class=\"hdrright\">{release}</td></tr></table></td></tr>\n\
             <tr><td class=\"nav\" valign=\"top\">\n{nav}</td>\n\
             <td class=\"content\" valign=\"top\">\n<div class=\"crumb\">{crumb}</div>\n\
             <h1>{title}</h1>\n{desc}{body}</td></tr>\n\
             <tr><td class=\"footer\" colspan=\"2\">QS/400 {release} &middot; Quartz Systems</td></tr>\n\
             </table>\n</body>\n</html>\n",
            title = esc(self.title),
            release = release::RELEASE,
            body = self.body,
        )
    }
}
