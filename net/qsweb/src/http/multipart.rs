// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `multipart/form-data` bodies: certificate uploads.

/// One part of the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// The field name.
    pub name: String,
    /// The uploaded file's name, for a file field.
    pub filename: Option<String>,
    /// The part's `Content-Type`.
    pub content_type: Option<String>,
    /// The bytes.
    pub data: Vec<u8>,
}

/// The boundary from a `Content-Type` header, if it is multipart.
pub fn boundary(content_type: &str) -> Option<String> {
    let mut it = content_type.split(';');
    if !it
        .next()?
        .trim()
        .eq_ignore_ascii_case("multipart/form-data")
    {
        return None;
    }
    it.filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| k.eq_ignore_ascii_case("boundary"))
        .map(|(_, v)| v.trim_matches('"').to_string())
        .filter(|b| !b.is_empty())
}

/// Split `body` at `--boundary` lines; `None` when it is not well formed.
pub fn parse(body: &[u8], boundary: &str) -> Option<Vec<Part>> {
    let delim = format!("--{boundary}").into_bytes();
    let mut at = find(body, &delim, 0)?;
    let mut parts = Vec::new();
    loop {
        at += delim.len();
        if body[at..].starts_with(b"--") {
            return Some(parts);
        }
        at = skip_eol(body, at)?;
        let head_end = find(body, b"\r\n\r\n", at)?;
        let head = std::str::from_utf8(&body[at..head_end]).ok()?;
        let data_start = head_end + 4;
        let mut next_delim = b"\r\n".to_vec();
        next_delim.extend_from_slice(&delim);
        let data_end = find(body, &next_delim, data_start)?;
        parts.push(part(head, body[data_start..data_end].to_vec())?);
        at = data_end + 2;
    }
}

fn part(head: &str, data: Vec<u8>) -> Option<Part> {
    let mut name = None;
    let mut filename = None;
    let mut content_type = None;
    for l in head.split("\r\n") {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case("content-disposition") {
            for p in v.split(';').skip(1) {
                if let Some((pk, pv)) = p.trim().split_once('=') {
                    let pv = pv.trim().trim_matches('"').to_string();
                    match pk.trim() {
                        "name" => name = Some(pv),
                        "filename" => filename = Some(pv),
                        _ => {}
                    }
                }
            }
        } else if k.trim().eq_ignore_ascii_case("content-type") {
            content_type = Some(v.trim().to_string());
        }
    }
    Some(Part {
        name: name?,
        filename,
        content_type,
        data,
    })
}

fn skip_eol(b: &[u8], at: usize) -> Option<usize> {
    if b[at..].starts_with(b"\r\n") {
        Some(at + 2)
    } else {
        None
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_fields_and_files() {
        let ct = "multipart/form-data; boundary=\"XyZ\"";
        let b = boundary(ct).unwrap();
        let body = b"--XyZ\r\nContent-Disposition: form-data; name=\"label\"\r\n\r\nweb\r\n\
--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.pem\"\r\nContent-Type: application/x-pem-file\r\n\r\n\
-----BEGIN\r\n\x00\xff\r\n--XyZ--\r\n";
        let parts = parse(body, &b).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(
            (parts[0].name.as_str(), parts[0].data.as_slice()),
            ("label", &b"web"[..])
        );
        assert_eq!(parts[1].filename.as_deref(), Some("a.pem"));
        assert_eq!(parts[1].data, b"-----BEGIN\r\n\x00\xff");
        assert_eq!(boundary("text/plain"), None);
        assert_eq!(parse(b"garbage", "XyZ"), None);
    }
}
