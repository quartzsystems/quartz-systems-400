// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IPP/1.1 (RFC 8010, RFC 8011): one `Print-Job` request per file,
//! sent by POST over HTTP/1.1 to the printer URI, `ipps` and `https` through
//! the [`TlsConnector`]. The request carries the operation attributes
//! every printer needs (charset, language, printer URI, user, job
//! name, document format) and the job's `copies`, `sides` and
//! `media`. The answer's status code decides; its `job-id` is kept.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

use super::{Endpoint, Receipt, Sink, SinkError, SinkErrorKind, Stream, TlsConnector};
use crate::job::{Duplex, PrintJob};
use crate::transform::{Format, Output, page};

/// IPP's port.
pub const PORT: u16 = 631;

/// The resource used when a URI names none.
pub const DEFAULT_PATH: &str = "/ipp/print";

/// `Print-Job`.
pub const PRINT_JOB: u16 = 0x0002;

/// Delimiter and value tags.
pub mod tag {
    /// operation-attributes-tag.
    pub const OPERATION: u8 = 0x01;
    /// job-attributes-tag.
    pub const JOB: u8 = 0x02;
    /// end-of-attributes-tag.
    pub const END: u8 = 0x03;
    /// unsupported-attributes-tag.
    pub const UNSUPPORTED: u8 = 0x05;
    /// integer.
    pub const INTEGER: u8 = 0x21;
    /// enum.
    pub const ENUM: u8 = 0x23;
    /// nameWithoutLanguage.
    pub const NAME: u8 = 0x42;
    /// textWithoutLanguage.
    pub const TEXT: u8 = 0x41;
    /// keyword.
    pub const KEYWORD: u8 = 0x44;
    /// uri.
    pub const URI: u8 = 0x45;
    /// charset.
    pub const CHARSET: u8 = 0x47;
    /// naturalLanguage.
    pub const LANGUAGE: u8 = 0x48;
    /// mimeMediaType.
    pub const MIME: u8 = 0x49;
}

/// A printer URI taken apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uri {
    /// TLS: `ipps` or `https`.
    pub secure: bool,
    /// Host.
    pub host: String,
    /// Port; 631 when none is given.
    pub port: u16,
    /// The resource, from `/`.
    pub path: String,
}

impl Uri {
    /// Parse `ipp://host[:port]/path` (also `ipps`, `http`, `https`),
    /// or a bare host, which gets the default path. `secure` forces
    /// TLS (`SECURECNN(*YES)`).
    pub fn parse(s: &str, secure: bool) -> Result<Self, SinkError> {
        let s = s.trim();
        let bad = || SinkError::new(SinkErrorKind::Connect, format!("{s}: not a printer URI"));
        let (scheme_secure, rest) = match s.split_once("://") {
            Some((sch, rest)) => match sch.to_ascii_lowercase().as_str() {
                "ipp" | "http" => (false, rest),
                "ipps" | "https" => (true, rest),
                _ => return Err(bad()),
            },
            None => (false, s),
        };
        let (auth, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, DEFAULT_PATH.to_string()),
        };
        let (host, port) = if let Some(v6) = auth.strip_prefix('[') {
            let (h, after) = v6.split_once(']').ok_or_else(bad)?;
            let port = after.strip_prefix(':').map(str::parse).transpose().map_err(|_| bad())?;
            (h.to_string(), port)
        } else {
            match auth.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), Some(p.parse().map_err(|_| bad())?)),
                None => (auth.to_string(), None),
            }
        };
        if host.is_empty() {
            return Err(bad());
        }
        Ok(Self {
            secure: secure || scheme_secure,
            host,
            port: port.unwrap_or(PORT),
            path,
        })
    }

    /// As the `printer-uri` attribute writes it.
    #[must_use]
    pub fn text(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let scheme = if self.secure { "ipps" } else { "ipp" };
        format!("{scheme}://{host}:{}{}", self.port, self.path)
    }
}

/// One attribute value (a set of values repeats the attribute with an
/// empty name; not needed here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// The value tag.
    pub tag: u8,
    /// The name.
    pub name: String,
    /// The value's bytes.
    pub value: Vec<u8>,
}

impl Attr {
    /// A string-valued attribute.
    #[must_use]
    pub fn text(tag: u8, name: &str, value: &str) -> Self {
        Self {
            tag,
            name: name.into(),
            value: value.as_bytes().to_vec(),
        }
    }

    /// An integer or enum attribute.
    #[must_use]
    pub fn int(tag: u8, name: &str, value: i32) -> Self {
        Self {
            tag,
            name: name.into(),
            value: value.to_be_bytes().to_vec(),
        }
    }

    /// The value as a string.
    #[must_use]
    pub fn as_str(&self) -> String {
        String::from_utf8_lossy(&self.value).to_string()
    }

    /// The value as an integer, when it is four bytes.
    #[must_use]
    pub fn as_int(&self) -> Option<i32> {
        <[u8; 4]>::try_from(self.value.as_slice()).ok().map(i32::from_be_bytes)
    }
}

/// An attribute group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The delimiter tag.
    pub tag: u8,
    /// Its attributes.
    pub attrs: Vec<Attr>,
}

/// An IPP request or response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Version, `(1, 1)`.
    pub version: (u8, u8),
    /// Operation id of a request, status code of a response.
    pub code: u16,
    /// request-id.
    pub request_id: u32,
    /// The groups.
    pub groups: Vec<Group>,
    /// The document data after the attributes.
    pub data: Vec<u8>,
}

impl Message {
    /// The encoding.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.version.0, self.version.1];
        out.extend_from_slice(&self.code.to_be_bytes());
        out.extend_from_slice(&self.request_id.to_be_bytes());
        for g in &self.groups {
            out.push(g.tag);
            for a in &g.attrs {
                out.push(a.tag);
                out.extend_from_slice(&u16::try_from(a.name.len()).unwrap_or(0).to_be_bytes());
                out.extend_from_slice(a.name.as_bytes());
                out.extend_from_slice(&u16::try_from(a.value.len()).unwrap_or(0).to_be_bytes());
                out.extend_from_slice(&a.value);
            }
        }
        out.push(tag::END);
        out.extend_from_slice(&self.data);
        out
    }

    /// Parse one message; the bytes after the end tag are its data.
    #[allow(clippy::many_single_char_names)]
    pub fn parse(b: &[u8]) -> Result<Self, SinkError> {
        let bad = |w: &str| SinkError::new(SinkErrorKind::Protocol, format!("IPP message: {w}"));
        if b.len() < 9 {
            return Err(bad("too short"));
        }
        let mut m = Self {
            version: (b[0], b[1]),
            code: u16::from_be_bytes([b[2], b[3]]),
            request_id: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            groups: Vec::new(),
            data: Vec::new(),
        };
        let mut i = 8;
        let u16_at = |i: usize| -> Result<usize, SinkError> {
            b.get(i..i + 2)
                .map(|s| usize::from(u16::from_be_bytes([s[0], s[1]])))
                .ok_or_else(|| bad("truncated"))
        };
        loop {
            let t = *b.get(i).ok_or_else(|| bad("no end tag"))?;
            i += 1;
            if t == tag::END {
                break;
            }
            if t < 0x10 {
                m.groups.push(Group {
                    tag: t,
                    attrs: Vec::new(),
                });
                continue;
            }
            let g = m.groups.last_mut().ok_or_else(|| bad("value before a group"))?;
            let nl = u16_at(i)?;
            let name = b.get(i + 2..i + 2 + nl).ok_or_else(|| bad("truncated name"))?;
            i += 2 + nl;
            let vl = u16_at(i)?;
            let value = b.get(i + 2..i + 2 + vl).ok_or_else(|| bad("truncated value"))?;
            i += 2 + vl;
            g.attrs.push(Attr {
                tag: t,
                name: String::from_utf8_lossy(name).to_string(),
                value: value.to_vec(),
            });
        }
        m.data = b[i..].to_vec();
        Ok(m)
    }

    /// The first attribute called `name`.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.groups.iter().flat_map(|g| &g.attrs).find(|a| a.name == name)
    }
}

/// The IPP `sides` keyword for `DUPLEX`.
#[must_use]
pub fn sides(d: Duplex) -> &'static str {
    match d {
        Duplex::No => "one-sided",
        Duplex::Yes => "two-sided-long-edge",
        Duplex::Tumble => "two-sided-short-edge",
    }
}

/// The `Print-Job` request for `doc`.
#[must_use]
pub fn print_job(uri: &Uri, job: &PrintJob, doc: &Output, request_id: u32) -> Message {
    let mut op = vec![
        Attr::text(tag::CHARSET, "attributes-charset", "utf-8"),
        Attr::text(tag::LANGUAGE, "attributes-natural-language", "en"),
        Attr::text(tag::URI, "printer-uri", &uri.text()),
        Attr::text(tag::NAME, "requesting-user-name", &job.job.1),
        Attr::text(tag::NAME, "job-name", &job.title()),
        Attr::text(tag::MIME, "document-format", doc.format.mime()),
    ];
    if doc.format == Format::Pdf || doc.format == Format::Text {
        op.push(Attr::text(tag::NAME, "document-name", &job.file));
    }
    let mut jobattrs = vec![
        Attr::int(tag::INTEGER, "copies", i32::from(job.copies.max(1))),
        Attr::text(tag::KEYWORD, "sides", sides(job.duplex)),
    ];
    // The paper the PCL transform chose; IPP printers pick it from the
    // keyword, PCL ones from the data.
    jobattrs.push(Attr::text(tag::KEYWORD, "media", page::layout(job).paper.media));
    Message {
        version: (1, 1),
        code: PRINT_JOB,
        request_id,
        groups: vec![
            Group {
                tag: tag::OPERATION,
                attrs: op,
            },
            Group {
                tag: tag::JOB,
                attrs: jobattrs,
            },
        ],
        data: doc.data.clone(),
    }
}

/// An HTTP response: status and body.
fn read_response(s: &mut dyn Stream) -> Result<(u16, Vec<u8>), SinkError> {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| SinkError::io(&e))?;
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| SinkError::new(SinkErrorKind::Protocol, format!("HTTP status line {line:?}")))?;
    let (mut len, mut chunked) = (None, false);
    loop {
        line.clear();
        r.read_line(&mut line).map_err(|e| SinkError::io(&e))?;
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            let v = v.trim();
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.parse::<usize>().ok(),
                "transfer-encoding" => chunked = v.eq_ignore_ascii_case("chunked"),
                _ => {}
            }
        }
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            line.clear();
            r.read_line(&mut line).map_err(|e| SinkError::io(&e))?;
            let n = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| SinkError::new(SinkErrorKind::Protocol, "HTTP chunk size"))?;
            if n == 0 {
                break;
            }
            let mut c = vec![0; n + 2];
            r.read_exact(&mut c).map_err(|e| SinkError::io(&e))?;
            body.extend_from_slice(&c[..n]);
        }
    } else if let Some(n) = len {
        body.resize(n, 0);
        r.read_exact(&mut body).map_err(|e| SinkError::io(&e))?;
    } else {
        r.read_to_end(&mut body).map_err(|e| SinkError::io(&e))?;
    }
    Ok((status, body))
}

/// An IPP printer.
#[derive(Clone)]
pub struct IppSink {
    /// The printer URI.
    pub uri: Uri,
    /// How long anything may take.
    pub timeout: std::time::Duration,
    /// The TLS engine for `ipps`.
    pub tls: Arc<dyn TlsConnector>,
    /// The next request-id.
    pub request_id: u32,
}

impl IppSink {
    /// A printer at `uri`.
    #[must_use]
    pub fn new(uri: Uri, tls: Arc<dyn TlsConnector>) -> Self {
        Self {
            uri,
            timeout: super::TIMEOUT,
            tls,
            request_id: 1,
        }
    }
}

impl Sink for IppSink {
    fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError> {
        let body = print_job(&self.uri, job, doc, self.request_id).encode();
        self.request_id = self.request_id.wrapping_add(1);
        let ep = Endpoint {
            timeout: self.timeout,
            ..Endpoint::new(&self.uri.host, self.uri.port)
        };
        let tcp = ep.connect()?;
        let mut s: Box<dyn Stream> = if self.uri.secure {
            self.tls
                .connect(&self.uri.host, tcp)
                .map_err(|e| SinkError::new(SinkErrorKind::Tls, e.to_string()))?
        } else {
            Box::new(tcp)
        };
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nUser-Agent: QS400-print\r\nConnection: close\r\n\r\n",
            self.uri.path,
            self.uri.host,
            self.uri.port,
            body.len()
        );
        s.write_all(head.as_bytes()).map_err(|e| SinkError::io(&e))?;
        s.write_all(&body).map_err(|e| SinkError::io(&e))?;
        s.flush().map_err(|e| SinkError::io(&e))?;
        let (status, reply) = read_response(s.as_mut())?;
        if status != 200 {
            return Err(SinkError::new(
                SinkErrorKind::Protocol,
                format!("HTTP status {status} from {}", self.uri.text()),
            ));
        }
        let m = Message::parse(&reply)?;
        if m.code >= 0x0400 {
            // Client errors: this file will not print there.
            let kind = if m.code < 0x0500 {
                SinkErrorKind::Rejected
            } else {
                SinkErrorKind::Protocol
            };
            return Err(SinkError::new(kind, format!("IPP status 0x{:04X}", m.code)));
        }
        let job_id = m.attr("job-id").and_then(Attr::as_int).map(|n| n.to_string());
        Ok(Receipt {
            bytes: doc.data.len() as u64,
            detail: format!(
                "IPP job {} at {}",
                job_id.as_deref().unwrap_or("?"),
                self.uri.text()
            ),
            job_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris() {
        let u = Uri::parse("ipp://lj4.example:6310/printers/a", false).unwrap();
        assert_eq!((u.host.as_str(), u.port, u.path.as_str(), u.secure), ("lj4.example", 6310, "/printers/a", false));
        let u = Uri::parse("10.0.0.5", true).unwrap();
        assert_eq!(u.text(), "ipps://10.0.0.5:631/ipp/print");
        let u = Uri::parse("https://[fe80::1]/ipp", false).unwrap();
        assert_eq!(u.text(), "ipps://[fe80::1]:631/ipp");
        assert!(Uri::parse("lpd://x", false).is_err());
    }

    #[test]
    fn round_trip() {
        let uri = Uri::parse("ipp://p", false).unwrap();
        let doc = Output {
            data: b"%PDF-1.4".to_vec(),
            format: Format::Pdf,
        };
        let job = PrintJob {
            copies: 3,
            duplex: Duplex::Tumble,
            ..PrintJob::default()
        };
        let m = print_job(&uri, &job, &doc, 7);
        let back = Message::parse(&m.encode()).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.attr("copies").unwrap().as_int(), Some(3));
        assert_eq!(back.attr("sides").unwrap().as_str(), "two-sided-short-edge");
        assert_eq!(back.attr("document-format").unwrap().as_str(), "application/pdf");
        assert_eq!(back.attr("printer-uri").unwrap().as_str(), "ipp://p:631/ipp/print");
        assert_eq!(back.data, b"%PDF-1.4");
    }
}
