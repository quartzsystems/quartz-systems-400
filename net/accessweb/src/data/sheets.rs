// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Spreadsheets and documents: XLSX and ODS both ways (the first sheet
//! read), the Excel-compatible XML, HTML and PDF written. Numbers are
//! written with the system's own digits, never through floating point.

use std::fmt::Write as _;

use qsh_utils::archive::zip::{self, Entry};

use super::xml::{self, Node, esc};
use crate::env::db::Val;

/// A zip entry with a fixed time, so files are the same for the same
/// rows (01/01/2026).
fn entry(name: &str, data: &[u8], deflate: bool) -> Entry {
    Entry::new(name, data, deflate, (0, (46 << 9) | (1 << 5) | 1), None)
}

/// `A`, `B`, ... `AA` for a column from 0.
fn col_name(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + u8::try_from(i % 26).unwrap_or(0));
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// The column from 0 of a cell reference (`C7`).
fn col_of(r: &str) -> usize {
    let mut n = 0usize;
    for c in r.bytes().take_while(u8::is_ascii_alphabetic) {
        n = n * 26 + usize::from(c.to_ascii_uppercase() - b'A') + 1;
    }
    n.saturating_sub(1)
}

/// An XLSX workbook of one sheet.
pub fn xlsx(header: Option<&[String]>, rows: &[Vec<Val>], sheet: &str) -> Vec<u8> {
    let mut data = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    let mut r = 0usize;
    let mut row = |cells: Vec<Val>, data: &mut String| {
        r += 1;
        let _ = write!(data, "<row r=\"{r}\">");
        for (i, v) in cells.iter().enumerate() {
            let at = format!("{}{r}", col_name(i));
            match v {
                Val::Null => {}
                Val::Num(n) => {
                    let _ = write!(data, "<c r=\"{at}\"><v>{}</v></c>", esc(n));
                }
                other => {
                    let _ = write!(
                        data,
                        "<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                        esc(&other.text())
                    );
                }
            }
        }
        data.push_str("</row>");
    };
    if let Some(h) = header {
        row(h.iter().map(|n| Val::Text(n.clone())).collect(), &mut data);
    }
    for x in rows {
        row(x.clone(), &mut data);
    }
    data.push_str("</sheetData></worksheet>");
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let book = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        esc(sheet)
    );
    let book_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>";
    zip::write(&[
        entry("[Content_Types].xml", types.as_bytes(), true),
        entry("_rels/.rels", rels.as_bytes(), true),
        entry("xl/workbook.xml", book.as_bytes(), true),
        entry("xl/_rels/workbook.xml.rels", book_rels.as_bytes(), true),
        entry("xl/worksheets/sheet1.xml", data.as_bytes(), true),
    ])
}

fn part(entries: &[Entry], name: &str) -> Result<Option<String>, String> {
    match entries.iter().find(|e| e.name == name) {
        None => Ok(None),
        Some(e) => {
            let b = e.plain()?;
            String::from_utf8(b)
                .map(Some)
                .map_err(|_| format!("{name} is not UTF-8"))
        }
    }
}

/// The text inside the element that starts at `nodes[i]`.
fn inner_text(nodes: &[Node], i: usize, element: &str) -> (String, usize) {
    let mut s = String::new();
    let mut j = i + 1;
    while j < nodes.len() {
        match &nodes[j] {
            Node::Close(n) if n == element => break,
            Node::Text(t) => s.push_str(t),
            _ => {}
        }
        j += 1;
    }
    (s, j)
}

/// The first sheet of an XLSX workbook, as rows.
pub fn read_xlsx(b: &[u8]) -> Result<Vec<Vec<Val>>, String> {
    let entries = zip::read(b)?;
    let shared: Vec<String> = match part(&entries, "xl/sharedStrings.xml")? {
        None => Vec::new(),
        Some(doc) => {
            let nodes = xml::parse(&doc);
            let mut out = Vec::new();
            let mut i = 0;
            while i < nodes.len() {
                if matches!(&nodes[i], Node::Open { name, empty: false, .. } if name == "si") {
                    let (t, j) = inner_text(&nodes, i, "si");
                    out.push(t);
                    i = j;
                }
                i += 1;
            }
            out
        }
    };
    // The first sheet the workbook lists, through its relationship.
    let mut target = "xl/worksheets/sheet1.xml".to_string();
    if let (Some(book), Some(rels)) = (
        part(&entries, "xl/workbook.xml")?,
        part(&entries, "xl/_rels/workbook.xml.rels")?,
    ) {
        let rid = xml::parse(&book).iter().find_map(|n| match n {
            Node::Open { name, .. } if name == "sheet" => n.attr("r:id").map(ToString::to_string),
            _ => None,
        });
        if let Some(rid) = rid
            && let Some(t) = xml::parse(&rels).iter().find_map(|n| {
                (n.attr("Id") == Some(&rid))
                    .then(|| n.attr("Target").map(ToString::to_string))
                    .flatten()
            })
        {
            target = if let Some(abs) = t.strip_prefix('/') {
                abs.to_string()
            } else {
                format!("xl/{t}")
            };
        }
    }
    let doc = part(&entries, &target)?.ok_or("the workbook has no sheet")?;
    let nodes = xml::parse(&doc);
    let mut rows: Vec<Vec<Val>> = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Open { name, .. } if name == "row" => rows.push(Vec::new()),
            Node::Open { name, empty, .. } if name == "c" => {
                let at = nodes[i]
                    .attr("r")
                    .map_or_else(|| rows.last().map_or(0, Vec::len), col_of);
                let t = nodes[i].attr("t").unwrap_or("n").to_string();
                let (mut v, mut is) = (String::new(), String::new());
                let mut has_v = false;
                if !*empty {
                    let mut j = i + 1;
                    while j < nodes.len() && !matches!(&nodes[j], Node::Close(n) if n == "c") {
                        if let Node::Open {
                            name, empty: false, ..
                        } = &nodes[j]
                        {
                            if name == "v" {
                                let (s, k) = inner_text(&nodes, j, "v");
                                (v, has_v) = (s, true);
                                j = k;
                            } else if name == "is" {
                                let (s, k) = inner_text(&nodes, j, "is");
                                is = s;
                                j = k;
                            }
                        }
                        j += 1;
                    }
                    i = j;
                }
                let val = match t.as_str() {
                    "s" => Val::Text(
                        v.trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|k| shared.get(k).cloned())
                            .unwrap_or_default(),
                    ),
                    "inlineStr" => Val::Text(is),
                    "str" | "e" => Val::Text(v),
                    "b" => Val::Text(if v.trim() == "1" {
                        "1".into()
                    } else {
                        "0".into()
                    }),
                    _ if has_v => Val::Num(v.trim().to_string()),
                    _ => Val::Null,
                };
                if let Some(row) = rows.last_mut() {
                    while row.len() < at {
                        row.push(Val::Null);
                    }
                    row.push(val);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(rows)
}

/// An ODS document of one sheet.
pub fn ods(header: Option<&[String]>, rows: &[Vec<Val>], sheet: &str) -> Vec<u8> {
    let mut body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\"><office:body><office:spreadsheet><table:table table:name=\"{}\">",
        esc(sheet)
    );
    let row = |cells: Vec<Val>, body: &mut String| {
        body.push_str("<table:table-row>");
        for v in &cells {
            match v {
                Val::Null => body.push_str("<table:table-cell/>"),
                Val::Num(n) => {
                    let _ = write!(
                        body,
                        "<table:table-cell office:value-type=\"float\" office:value=\"{0}\"><text:p>{0}</text:p></table:table-cell>",
                        esc(n)
                    );
                }
                other => {
                    let _ = write!(
                        body,
                        "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        esc(&other.text())
                    );
                }
            }
        }
        body.push_str("</table:table-row>");
    };
    if let Some(h) = header {
        row(h.iter().map(|n| Val::Text(n.clone())).collect(), &mut body);
    }
    for x in rows {
        row(x.clone(), &mut body);
    }
    body.push_str("</table:table></office:spreadsheet></office:body></office:document-content>");
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.spreadsheet\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>";
    zip::write(&[
        // The media type first and stored, as the format says.
        entry(
            "mimetype",
            b"application/vnd.oasis.opendocument.spreadsheet",
            false,
        ),
        entry("META-INF/manifest.xml", manifest.as_bytes(), true),
        entry("content.xml", body.as_bytes(), true),
    ])
}

/// The cap on repeated empty cells and rows an ODS file may stand for.
const REPEAT_CAP: usize = 1024;

/// The first sheet of an ODS document, as rows.
pub fn read_ods(b: &[u8]) -> Result<Vec<Vec<Val>>, String> {
    let entries = zip::read(b)?;
    let doc = part(&entries, "content.xml")?.ok_or("the document has no content.xml")?;
    let nodes = xml::parse(&doc);
    let mut rows: Vec<Vec<Val>> = Vec::new();
    let mut tables = 0;
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Open { name, .. } if name == "table:table" => {
                tables += 1;
                if tables > 1 {
                    break;
                }
            }
            Node::Open { name, empty, .. } if name == "table:table-row" && tables == 1 => {
                let repeat = nodes[i]
                    .attr("table:number-rows-repeated")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(1usize);
                let mut row = Vec::new();
                let mut j = i + 1;
                if !*empty {
                    while j < nodes.len()
                        && !matches!(&nodes[j], Node::Close(n) if n == "table:table-row")
                    {
                        if let Node::Open { name, empty, .. } = &nodes[j]
                            && (name == "table:table-cell" || name == "table:covered-table-cell")
                        {
                            let n = nodes[j]
                                .attr("table:number-columns-repeated")
                                .and_then(|r| r.parse().ok())
                                .unwrap_or(1usize);
                            let typ = nodes[j].attr("office:value-type").unwrap_or("").to_string();
                            let value = nodes[j].attr("office:value").map(ToString::to_string);
                            let mut text = String::new();
                            if !*empty {
                                let mut k = j + 1;
                                let mut paras = 0;
                                while k < nodes.len()
                                    && !matches!(&nodes[k], Node::Close(n) if n == name.as_str())
                                {
                                    match &nodes[k] {
                                        Node::Open { name, .. } if name == "text:p" => {
                                            if paras > 0 {
                                                text.push('\n');
                                            }
                                            paras += 1;
                                        }
                                        Node::Open { name, .. } if name == "text:s" => {
                                            text.push(' ')
                                        }
                                        Node::Text(t) => text.push_str(t),
                                        _ => {}
                                    }
                                    k += 1;
                                }
                                j = k;
                            }
                            let v = match (typ.as_str(), value) {
                                ("float" | "percentage" | "currency", Some(v)) => Val::Num(v),
                                ("", _) if text.is_empty() => Val::Null,
                                _ => Val::Text(text),
                            };
                            for _ in 0..n.min(REPEAT_CAP) {
                                row.push(v.clone());
                            }
                        }
                        j += 1;
                    }
                }
                while row.last().is_some_and(Val::is_null) {
                    row.pop();
                }
                if !row.is_empty() {
                    for _ in 0..repeat.min(REPEAT_CAP) {
                        rows.push(row.clone());
                    }
                }
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(rows)
}

/// The Excel-compatible download: an XML spreadsheet (`.xls`).
pub fn xls(header: &[String], rows: &[Vec<Val>], sheet: &str) -> Vec<u8> {
    let mut s = format!(
        "<?xml version=\"1.0\"?>\n<?mso-application progid=\"Excel.Sheet\"?>\n<Workbook xmlns=\"urn:schemas-microsoft-com:office:spreadsheet\" xmlns:ss=\"urn:schemas-microsoft-com:office:spreadsheet\"><Worksheet ss:Name=\"{}\"><Table>",
        esc(sheet)
    );
    s.push_str("<Row>");
    for h in header {
        let _ = write!(s, "<Cell><Data ss:Type=\"String\">{}</Data></Cell>", esc(h));
    }
    s.push_str("</Row>");
    for r in rows {
        s.push_str("<Row>");
        for v in r {
            match v {
                Val::Null => s.push_str("<Cell/>"),
                Val::Num(n) => {
                    let _ = write!(s, "<Cell><Data ss:Type=\"Number\">{}</Data></Cell>", esc(n));
                }
                other => {
                    let _ = write!(
                        s,
                        "<Cell><Data ss:Type=\"String\">{}</Data></Cell>",
                        esc(&other.text())
                    );
                }
            }
        }
        s.push_str("</Row>");
    }
    s.push_str("</Table></Worksheet></Workbook>\n");
    s.into_bytes()
}

/// A page of its own with the rows in a table.
pub fn html(title: &str, header: &[String], numeric: &[bool], rows: &[Vec<Val>]) -> Vec<u8> {
    let e = qsweb::html::esc;
    let mut s = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{}</title></head><body>\n<table border=\"1\" cellspacing=\"0\" cellpadding=\"2\">\n<tr>",
        e(title)
    );
    for h in header {
        let _ = write!(s, "<th>{}</th>", e(h));
    }
    s.push_str("</tr>\n");
    for r in rows {
        s.push_str("<tr>");
        for (v, num) in r
            .iter()
            .zip(numeric.iter().chain(std::iter::repeat(&false)))
        {
            let align = if *num { " align=\"right\"" } else { "" };
            match v {
                Val::Null => {
                    let _ = write!(s, "<td{align}>-</td>");
                }
                other => {
                    let _ = write!(s, "<td{align}>{}</td>", e(&other.text()));
                }
            }
        }
        s.push_str("</tr>\n");
    }
    s.push_str("</table>\n</body></html>\n");
    s.into_bytes()
}

/// A PDF of fixed-width text, landscape, as many lines a page as fit.
pub fn pdf(text: &str) -> Vec<u8> {
    use std::collections::BTreeMap;
    let width = text
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .clamp(80, 198);
    let fmt = spool::scs::decode::Format {
        width: u16::try_from(width).unwrap_or(198),
        len: 51,
        lpi: 6,
        cpi: if width > 132 { 15 } else { 10 },
    };
    let lines: Vec<&str> = text.lines().collect();
    let pages: Vec<spool::scs::decode::Page> = lines
        .chunks(50)
        .map(|chunk| {
            let lines: BTreeMap<u16, String> = chunk
                .iter()
                .enumerate()
                .map(|(i, l)| (u16::try_from(i + 1).unwrap_or(1), (*l).to_string()))
                .collect();
            spool::scs::decode::Page {
                lines,
                passes: BTreeMap::new(),
            }
        })
        .collect();
    spool::pdf::render(&pages, fmt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Vec<Val>> {
        vec![
            vec![
                Val::Text("QS000101".into()),
                Val::Num("1200.05".into()),
                Val::Null,
            ],
            vec![
                Val::Text("a & <b>".into()),
                Val::Num("-0.000000000000000000001".into()),
                Val::Text(String::new()),
            ],
        ]
    }

    #[test]
    fn xlsx_both_ways() {
        let h = vec!["TAG".to_string(), "COST".into(), "NOTES".into()];
        let back = read_xlsx(&xlsx(Some(&h), &rows(), "ASSETS")).unwrap();
        assert_eq!(
            back[0],
            vec![
                Val::Text("TAG".into()),
                Val::Text("COST".into()),
                Val::Text("NOTES".into())
            ]
        );
        assert_eq!(
            back[1],
            vec![Val::Text("QS000101".into()), Val::Num("1200.05".into())]
        );
        assert_eq!(back[2][1], Val::Num("-0.000000000000000000001".into()));
        assert_eq!(col_name(27), "AB");
        assert_eq!(col_of("AB12"), 27);
    }

    #[test]
    fn ods_both_ways() {
        let back = read_ods(&ods(None, &rows(), "ASSETS")).unwrap();
        assert_eq!(
            back[0],
            vec![Val::Text("QS000101".into()), Val::Num("1200.05".into())]
        );
        assert_eq!(back[1][0], Val::Text("a & <b>".into()));
    }

    #[test]
    fn documents() {
        let x = String::from_utf8(xls(&["N".into()], &[vec![Val::Num("4".into())]], "S")).unwrap();
        assert!(x.contains("<Data ss:Type=\"Number\">4</Data>"));
        let h = String::from_utf8(html("T", &["N".into()], &[true], &[vec![Val::Null]])).unwrap();
        assert!(h.contains("<td align=\"right\">-</td>"));
        assert!(pdf("ASSET_TAG\nQS000101\n").starts_with(b"%PDF"));
    }
}
