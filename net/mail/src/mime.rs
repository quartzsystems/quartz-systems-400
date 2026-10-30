// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message `SNDSMTPEMM` sends (RFC 5322 with MIME, RFC 2045-2049):
//! the note as text/plain or text/html in UTF-8, quoted-printable; with
//! attachments, a multipart/mixed whose first part is the note and the
//! rest the files, text ones quoted-printable and the others base64.
//! Blind copies are recipients, never headers.

use std::fmt::Write as _;

use crate::addr::Address;
use crate::encode::{base64_lines, date, quoted_printable, word};

/// The note: `CONTENT(*PLAIN)` or `CONTENT(*HTML)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// text/plain.
    Plain(String),
    /// text/html.
    Html(String),
}

/// A file sent with the note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// The file name the recipient sees.
    pub name: String,
    /// Its media type.
    pub content_type: String,
    /// Its bytes.
    pub data: Vec<u8>,
    /// Sent as text (UTF-8, quoted-printable) rather than base64.
    pub text: bool,
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The sender.
    pub from: Address,
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Copies.
    pub cc: Vec<Address>,
    /// Blind copies.
    pub bcc: Vec<Address>,
    /// The subject.
    pub subject: String,
    /// The note.
    pub body: Body,
    /// Files.
    pub attachments: Vec<Attachment>,
    /// When, Unix seconds.
    pub date: i64,
    /// The host name for `Message-ID`.
    pub host: String,
    /// Something unique to this message (job, time, count) for its
    /// identifier and boundary.
    pub unique: u64,
}

/// A media type from a file's extension; `application/octet-stream`
/// for the rest.
#[must_use]
pub fn content_type_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "xml" => "text/xml",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

fn list(addrs: &[Address]) -> String {
    addrs.iter().map(Address::header).collect::<Vec<_>>().join(",\r\n ")
}

/// A file name for a `name=` parameter: quoted, or RFC 2047 when not
/// ASCII (what mail programs read).
fn param(name: &str) -> String {
    let w = word(name);
    if w == name {
        format!("\"{}\"", name.replace(['"', '\\'], "_"))
    } else {
        format!("\"{w}\"")
    }
}

impl Message {
    /// Every recipient's mailbox, in order, once each.
    #[must_use]
    pub fn recipients(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in self.to.iter().chain(&self.cc).chain(&self.bcc) {
            if !out.iter().any(|m| m.eq_ignore_ascii_case(&a.mailbox)) {
                out.push(a.mailbox.clone());
            }
        }
        out
    }

    /// The boundary between parts.
    #[must_use]
    pub fn boundary(&self) -> String {
        format!("=_QS400_{:016X}", self.unique)
    }

    fn note(&self) -> String {
        let (kind, text) = match &self.body {
            Body::Plain(t) => ("plain", t),
            Body::Html(t) => ("html", t),
        };
        format!(
            "Content-Type: text/{kind}; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n{}\r\n",
            quoted_printable(text)
        )
    }

    fn part(a: &Attachment) -> String {
        let name = param(&a.name);
        let mut out = String::new();
        if a.text {
            let text = String::from_utf8_lossy(&a.data);
            let _ = write!(
                out,
                "Content-Type: {}; charset=utf-8; name={name}\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Disposition: attachment; filename={name}\r\n\r\n{}\r\n",
                a.content_type,
                quoted_printable(&text)
            );
        } else {
            let _ = write!(
                out,
                "Content-Type: {}; name={name}\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename={name}\r\n\r\n{}",
                a.content_type,
                base64_lines(&a.data)
            );
        }
        out
    }

    /// The message as sent: headers, a blank line, the body; CRLF line
    /// ends throughout.
    #[must_use]
    pub fn render(&self) -> Vec<u8> {
        let mut h = String::new();
        let _ = write!(h, "From: {}\r\n", self.from.header());
        if !self.to.is_empty() {
            let _ = write!(h, "To: {}\r\n", list(&self.to));
        }
        if !self.cc.is_empty() {
            let _ = write!(h, "Cc: {}\r\n", list(&self.cc));
        }
        let _ = write!(h, "Subject: {}\r\n", word(&self.subject));
        let _ = write!(h, "Date: {}\r\n", date(self.date));
        let _ = write!(h, "Message-ID: <{:016x}.{}@{}>\r\n", self.unique, self.date, self.host);
        h.push_str("MIME-Version: 1.0\r\n");
        if self.attachments.is_empty() {
            h.push_str(&self.note());
            return h.into_bytes();
        }
        let b = self.boundary();
        let _ = write!(h, "Content-Type: multipart/mixed; boundary=\"{b}\"\r\n\r\n");
        let _ = write!(h, "--{b}\r\n{}", self.note());
        for a in &self.attachments {
            let _ = write!(h, "--{b}\r\n{}", Self::part(a));
        }
        let _ = write!(h, "--{b}--\r\n");
        h.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Message {
        Message {
            from: Address::mailbox("qsecofr@qs400.example.com"),
            to: vec![Address::parse("Anne <anne@example.com>").unwrap()],
            cc: vec![Address::mailbox("bob@example.com")],
            bcc: vec![Address::mailbox("audit@example.com"), Address::mailbox("ANNE@example.com")],
            subject: "Nightly totals".into(),
            body: Body::Plain("All batches ended.\nTotal: 42".into()),
            attachments: Vec::new(),
            date: 1_793_363_696,
            host: "qs400.example.com".into(),
            unique: 0xABCD,
        }
    }

    #[test]
    fn a_plain_note() {
        let m = sample();
        let text = String::from_utf8(m.render()).unwrap();
        assert!(text.starts_with("From: qsecofr@qs400.example.com\r\nTo: Anne <anne@example.com>\r\nCc: bob@example.com\r\n"));
        assert!(text.contains("Subject: Nightly totals\r\nDate: Fri, 30 Oct 2026 12:34:56 +0000\r\n"));
        assert!(text.contains("Message-ID: <000000000000abcd.1793363696@qs400.example.com>\r\n"));
        assert!(text.contains("Content-Type: text/plain; charset=utf-8\r\n"));
        assert!(text.ends_with("\r\n\r\nAll batches ended.\r\nTotal: 42\r\n"));
        assert!(!text.contains("audit@"), "no Bcc header");
        assert_eq!(m.recipients(), ["anne@example.com", "bob@example.com", "audit@example.com"]);
    }

    #[test]
    fn html_with_attachments() {
        let mut m = sample();
        m.body = Body::Html("<p>See the files.</p>".into());
        m.attachments = vec![
            Attachment {
                name: "totals.csv".into(),
                content_type: content_type_for("totals.csv").into(),
                data: b"a,b\n1,2\n".to_vec(),
                text: true,
            },
            Attachment {
                name: "logo.png".into(),
                content_type: content_type_for("logo.png").into(),
                data: vec![0x89, b'P', b'N', b'G'],
                text: false,
            },
        ];
        let text = String::from_utf8(m.render()).unwrap();
        let b = m.boundary();
        assert!(text.contains(&format!("Content-Type: multipart/mixed; boundary=\"{b}\"\r\n\r\n--{b}\r\n")));
        assert!(text.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(text.contains("Content-Type: text/csv; charset=utf-8; name=\"totals.csv\"\r\n"));
        assert!(text.contains("a,b\r\n1,2\r\n"));
        assert!(text.contains("Content-Type: image/png; name=\"logo.png\"\r\nContent-Transfer-Encoding: base64\r\n"));
        assert!(text.contains("iVBORw==\r\n"));
        assert!(text.ends_with(&format!("--{b}--\r\n")));
        assert_eq!(content_type_for("X.PDF"), "application/pdf");
        assert_eq!(content_type_for("noext"), "application/octet-stream");
    }
}
