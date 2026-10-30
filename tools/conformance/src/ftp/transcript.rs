// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FTP transcripts (`tools/conformance/ftp/README.md`): `C:` commands,
//! `S:` reply lines, `D:` data received (`D|` text or hex lines), `D>`
//! data sent (with `U|` hex lines of the payload in the second round).

/// What a data connection brought from the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataIn {
    /// Text: the byte count and the lines (line ends off, at most the
    /// first 60 shown) and how many more there were.
    Text {
        /// The byte count.
        len: usize,
        /// The lines shown.
        lines: Vec<String>,
        /// How many lines were cut.
        more: usize,
    },
    /// Bytes, from hex.
    Hex(Vec<u8>),
}

/// One command and what came of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// The command as sent (`{user}` masked, the password starred).
    pub cmd: String,
    /// The reply lines, in order.
    pub replies: Vec<String>,
    /// What came in on the data connection.
    pub data: Option<DataIn>,
    /// What went out on it: the byte count and the payload when known.
    pub sent: Option<(usize, Option<Vec<u8>>)>,
}

/// One recorded session.
#[derive(Clone, Debug, Default)]
pub struct Transcript {
    /// The session's name.
    pub name: String,
    /// The banner's lines.
    pub banner: Vec<String>,
    /// The commands.
    pub steps: Vec<Step>,
    /// The oracle closed the connection or the recorder failed here:
    /// the steps after this index are not to be compared.
    pub cut: Option<usize>,
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// Read a transcript.
pub fn parse(name: &str, text: &str) -> Transcript {
    let mut t = Transcript {
        name: name.to_string(),
        ..Transcript::default()
    };
    let mut hexbuf: Option<Vec<u8>> = None;
    let mut textbuf: Option<(usize, Vec<String>, usize)> = None;
    let flush = |t: &mut Transcript, hexbuf: &mut Option<Vec<u8>>, textbuf: &mut Option<(usize, Vec<String>, usize)>| {
        if let Some(h) = hexbuf.take()
            && let Some(s) = t.steps.last_mut()
        {
            s.data = Some(DataIn::Hex(h));
        }
        if let Some((len, lines, more)) = textbuf.take()
            && let Some(s) = t.steps.last_mut()
        {
            s.data = Some(DataIn::Text { len, lines, more });
        }
    };
    for line in text.lines() {
        if let Some(c) = line.strip_prefix("C: ") {
            flush(&mut t, &mut hexbuf, &mut textbuf);
            t.steps.push(Step {
                cmd: c.to_string(),
                ..Step::default()
            });
        } else if let Some(s) = line.strip_prefix("S: ") {
            flush(&mut t, &mut hexbuf, &mut textbuf);
            if s == "<closed>" {
                if t.cut.is_none() {
                    t.cut = Some(t.steps.len().saturating_sub(1));
                }
                continue;
            }
            match t.steps.last_mut() {
                Some(st) => st.replies.push(s.to_string()),
                None => t.banner.push(s.to_string()),
            }
        } else if let Some(d) = line.strip_prefix("D: ") {
            flush(&mut t, &mut hexbuf, &mut textbuf);
            let len = d.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            if d.ends_with("(hex)") {
                hexbuf = Some(Vec::new());
            } else {
                textbuf = Some((len, Vec::new(), 0));
            }
        } else if let Some(d) = line.strip_prefix("D| ").or_else(|| line.strip_prefix("D|")) {
            if let Some(h) = &mut hexbuf {
                h.extend(hex(d.trim()));
            } else if let Some((_, lines, more)) = &mut textbuf {
                if let Some(n) = d.strip_prefix("... (").and_then(|r| r.strip_suffix(" more lines)")) {
                    *more = n.parse().unwrap_or(0);
                } else {
                    lines.push(d.to_string());
                }
            }
        } else if let Some(d) = line.strip_prefix("D> ") {
            flush(&mut t, &mut hexbuf, &mut textbuf);
            let n = d.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            if let Some(s) = t.steps.last_mut() {
                s.sent = Some((n, if n == 0 { Some(Vec::new()) } else { None }));
            }
        } else if let Some(u) = line.strip_prefix("U| ") {
            if let Some(Some((_, p))) = t.steps.last_mut().map(|s| s.sent.as_mut()) {
                p.get_or_insert_with(Vec::new).extend(hex(u.trim()));
            }
        } else if line.starts_with("# error") {
            flush(&mut t, &mut hexbuf, &mut textbuf);
            if t.cut.is_none() {
                t.cut = Some(t.steps.len().saturating_sub(1));
            }
        }
    }
    flush(&mut t, &mut hexbuf, &mut textbuf);
    t
}

/// The reply codes of a step, one per reply (the last line of each).
pub fn codes(lines: &[String]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut open: Option<u16> = None;
    for l in lines {
        let code = l.get(..3).and_then(|c| c.parse::<u16>().ok());
        let sep = l.as_bytes().get(3).copied();
        match (open, code) {
            (None, Some(c)) if sep == Some(b'-') => open = Some(c),
            (None, Some(c)) => out.push(c),
            (Some(o), Some(c)) if c == o && sep != Some(b'-') => {
                out.push(c);
                open = None;
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_transcript() {
        let t = parse(
            "x",
            "# session: x\nS: 220-a\nS: 220 b\nC: USER {user}\nS: 331 p\nC: RETR f\nS: 150 s\nD: 3 bytes (hex)\nD| 616263\nS: 226 ok\nC: STOR g\nS: 150 s\nD> 2 bytes sent\nU| 0102\nS: 226 ok\nC: LIST\nS: 125 l\nD: 4 bytes\nD| ab\nD| ... (3 more lines)\nS: 250 done\n",
        );
        assert_eq!(t.banner.len(), 2);
        assert_eq!(t.steps.len(), 4);
        assert_eq!(t.steps[1].data, Some(DataIn::Hex(b"abc".to_vec())));
        assert_eq!(t.steps[2].sent, Some((2, Some(vec![1, 2]))));
        assert_eq!(
            t.steps[3].data,
            Some(DataIn::Text {
                len: 4,
                lines: vec!["ab".into()],
                more: 3
            })
        );
        assert_eq!(codes(&t.steps[1].replies), [150, 226]);
        assert_eq!(codes(&["550-a".into(), "550 b".into()]), [550]);
    }
}
