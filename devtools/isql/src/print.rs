// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printed forms: the session (F13 option 2, `QSYSPRT`) and a
//! query's rows when SELECT output is 2 (`QPQUPRFIL`), page by page,
//! laid out as pub400 prints them
//! (`tools/conformance/golden/sql/strsql/print/`). The words come from
//! `QSTXTMSG` through [`Words`].

use crate::result::{Data, window};

/// The displays' and reports' words: the text of a `QSTXTMSG`
/// identifier in the job's language.
pub type Words<'a> = dyn Fn(&str) -> String + 'a;

/// The words of the printed forms.
pub mod txt {
    /// The session report's title.
    pub const SESSION_TITLE: &str = "TXT0901";
    /// `Page` in the session report's heading.
    pub const SESSION_PAGE: &str = "TXT0902";
    /// The session report's last line.
    pub const SESSION_END: &str = "TXT0903";
    /// `PAGE` in the query report's heading.
    pub const REPORT_PAGE: &str = "TXT0904";
    /// The query report's last line.
    pub const REPORT_END: &str = "TXT0905";
}

/// When a report was printed, as the job writes dates and times.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// The date.
    pub date: String,
    /// The time.
    pub time: String,
}

/// The session report: a heading on each page (product, release,
/// title, date, time, page), the session's lines, the end line.
#[must_use]
pub fn session(
    lines: &[String],
    product: &str,
    release: &str,
    stamp: &Stamp,
    per_page: usize,
    w: &Words<'_>,
) -> Vec<Vec<String>> {
    let head = |page: usize| {
        format!(
            "  {product:<10}{release:<7}{:<10}{:<26}{:<11}{:<19}{}{page:>8}",
            "",
            w(txt::SESSION_TITLE),
            stamp.date,
            stamp.time,
            w(txt::SESSION_PAGE),
        )
    };
    let mut body: Vec<String> = lines.to_vec();
    body.push(format!("{:32}{}", "", w(txt::SESSION_END)));
    paged(&body, per_page, head)
}

/// The query report: a heading on each page (date, time, page, the
/// page number on the data's last column), the column heading, the
/// rows and the end line; data wider than `width` continues in
/// further sections.
#[must_use]
pub fn select(
    data: &Data,
    stamp: &Stamp,
    width: usize,
    per_page: usize,
    w: &Words<'_>,
) -> Vec<Vec<String>> {
    let total = data.width();
    let mut body = Vec::new();
    let mut shift = 1;
    while shift <= total.max(1) {
        if shift > 1 {
            body.push(String::new());
        }
        body.push(window(&data.heading(), shift, width));
        for i in 0..data.rows.len() {
            body.push(window(&data.line(i), shift, width));
        }
        shift += width;
    }
    body.push(w(txt::REPORT_END));
    let span = total.min(width);
    let head = |page: usize| {
        let left = format!("{}  {}", stamp.date, stamp.time);
        let right = format!("{}{page:>5}", w(txt::REPORT_PAGE));
        let gap = span.saturating_sub(left.len() + right.len()).max(2);
        format!("{left}{:gap$}{right}", "")
    };
    paged(&body, per_page, head)
}

/// `body` cut into pages of `per_page` lines under `head(n)`.
fn paged(body: &[String], per_page: usize, head: impl Fn(usize) -> String) -> Vec<Vec<String>> {
    let per = per_page.max(2) - 1;
    body.chunks(per)
        .enumerate()
        .map(|(i, c)| {
            let mut p = vec![head(i + 1)];
            p.extend(c.iter().cloned());
            p
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::Column;

    fn words(id: &str) -> String {
        match id {
            txt::SESSION_TITLE => "Interactive SQL Session",
            txt::SESSION_PAGE => "Page",
            txt::SESSION_END => "* * * * *  E N D  O F  L I S T I N G  * * * * *",
            txt::REPORT_PAGE => "PAGE",
            txt::REPORT_END => "* * *  E N D  O F  R E P O R T  * * *",
            _ => id,
        }
        .to_string()
    }

    #[test]
    fn headings_as_printed() {
        let stamp = Stamp {
            date: "26-11-05".into(),
            time: "19:38:09".into(),
        };
        let p = session(
            &["select 1".into()],
            "5770ST1",
            "V7R5M0",
            &stamp,
            60,
            &words,
        );
        let h = &p[0][0];
        assert_eq!(h.find("Interactive"), Some(29));
        assert_eq!(h.find("26-11-05"), Some(55));
        assert_eq!(h.find("19:38:09"), Some(66));
        assert_eq!(h.find("Page"), Some(85));
        assert_eq!(h.len(), 97);
        assert_eq!(p[0].last().map(|l| l.find('*')), Some(Some(32)));
        let d = Data {
            columns: vec![Column {
                name: "C".into(),
                data_type: "DECIMAL(43, 0)".into(),
            }],
            rows: vec![],
            fmt: crate::format::Format::default(),
        };
        let stamp = Stamp {
            date: "26-11-05".into(),
            time: "19:38:21".into(),
        };
        let r = select(&d, &stamp, 132, 60, &words);
        assert_eq!(d.width(), 58);
        assert_eq!(r[0][0].len(), 58);
        assert!(r[0][0].ends_with("PAGE    1"));
    }
}
