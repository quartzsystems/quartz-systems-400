// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 155: Access for Web's database pages from live state, over
//! HTTP on 2003. `QSECOFR` signs on and Run SQL makes schema `AFW155`
//! with table `ITEMS` and a row, and reads it back (`SQL7956`, the
//! row); Tables lists `ITEMS`; the table's page shows the row; Insert
//! record adds one; Copy data to table copies a CSV file of two rows in
//! (`QAD7030`) and Copy data from table copies all four out again as
//! CSV, decimals exact. A profile without authority to the table runs
//! the same query and gets the database's `SQL0551`: statements run as
//! the signed-on user, not as the server's `QTMHHTTP`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::httpadm::{Reply, enc, http_at, secofr_password, session_cookie, want};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The product's URLs.
const PREFIX: &str = "/qs400/accessweb";

/// The profile without authority to the table, and its password.
const USER: &str = "AFW155";
const PASSWORD: &str = "AFW155PW";

/// The file copied in.
const CSV: &str = "ID,NAME,PRICE\r\n3,WASHER,0.05\r\n4,SCREW,1234567.89\r\n";

fn afw(port: u16, method: &str, path: &str, cookie: Option<&str>, form: Option<&str>) -> Result<Reply, String> {
    http_at(PREFIX, port, method, path, cookie, form)
}

/// The form field every POST carries.
fn csrf_in(body: &str) -> Option<String> {
    let at = body.find("name=\"csrf\" value=\"")? + "name=\"csrf\" value=\"".len();
    let end = body[at..].find('"')?;
    Some(body[at..at + end].to_string())
}

fn sign_on(port: u16, user: &str, password: &str) -> Result<String, String> {
    let form = format!("user={user}&password={}", enc(password));
    let ok = afw(port, "POST", "/signon", None, Some(&form))?;
    if ok.status != 303 {
        return Err(format!("signing on as {user}:\n{}", ok.brief()));
    }
    session_cookie(&ok).ok_or_else(|| format!("no session cookie:\n{}", ok.brief()))
}

fn token(port: u16, path: &str, sid: &str) -> Result<String, String> {
    let p = afw(port, "GET", path, Some(sid), None)?;
    csrf_in(&p.body).ok_or_else(|| format!("no form field on {path}:\n{}", p.brief()))
}

/// A multipart POST: text fields and one file.
fn multipart(port: u16, path: &str, sid: &str, fields: &[(&str, &str)], file: Option<(&str, &str, &[u8])>) -> Result<Reply, String> {
    let boundary = "----qs400step155";
    let mut body = Vec::new();
    for (k, v) in fields {
        body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    }
    if let Some((k, name, data)) = file {
        body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"; filename=\"{name}\"\r\nContent-Type: text/csv\r\n\r\n").as_bytes());
        body.extend_from_slice(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());
    let mut req = format!(
        "POST {PREFIX}{path} HTTP/1.1\r\nHost: qs400:2003\r\nConnection: close\r\nCookie: QS400SID={sid}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    req.extend(body);
    let mut c = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let _ = c.set_read_timeout(Some(Duration::from_secs(60)));
    c.write_all(&req).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let _ = c.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).into_owned();
    let (head, rest) = text.split_once("\r\n\r\n").ok_or_else(|| format!("POST {path}: no answer"))?;
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Reply {
        status,
        head: head.to_string(),
        body: rest.to_string(),
    })
}

/// The copy page `at` once its copy has ended.
fn finished(port: u16, at: &str, sid: &str) -> Result<Reply, String> {
    let end = Instant::now() + Duration::from_secs(120);
    loop {
        let p = afw(port, "GET", at, Some(sid), None)?;
        if !p.body.contains("name=\"cancel\" value=\"Cancel\"") || Instant::now() > end {
            return Ok(p);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Where a redirect goes, under the prefix.
fn location(r: &Reply) -> Option<String> {
    r.header("Location").and_then(|l| l.strip_prefix(PREFIX)).map(ToString::to_string)
}

/// The script Run SQL runs.
const SCRIPT: &str = "CREATE SCHEMA AFW155;\nCREATE TABLE AFW155.ITEMS (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(20) NOT NULL, PRICE DECIMAL(9,2));\nINSERT INTO AFW155.ITEMS VALUES (1, 'BOLT', 0.25);\nSELECT ID, NAME, PRICE FROM AFW155.ITEMS";

/// Run `script` on Run SQL: the page after.
fn run_sql(port: u16, sid: &str, script: &str) -> Result<Reply, String> {
    let t = token(port, "/sql", sid)?;
    let form = format!("csrf={}&schema=QGPL&naming=SQL&commit=None&stop=on&sql={}&runall=Run+all", enc(&t), enc(script));
    let r = afw(port, "POST", "/sql", Some(sid), Some(&form))?;
    want(&r, 303, &[], "Run SQL's Run all")?;
    afw(port, "GET", "/sql", Some(sid), None)
}

impl Suite {
    /// Step 155: Access for Web's database pages, live.
    pub(crate) fn access_web_db(&self, s: &mut Session) -> Result<String, String> {
        secofr_password(s)?;
        let _ = s.command("DLTLIB LIB(AFW155)", CMD)?;
        let _ = s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD)?;
        s.command(
            &format!("CRTUSRPRF USRPRF({USER}) PASSWORD({PASSWORD}) USRCLS(*USER) PWDEXP(*NO) TEXT('Step 155')"),
            CMD,
        )?;
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let started = s.command("STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
            if started.contains("TCP1A07") || (started.contains("TCP1A08") && Instant::now() > end) {
                break;
            }
            if !started.contains("TCP1A08") {
                return Err(format!("STRTCPSVR ACCESSWEB:\n{started}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let port = forward_port(self, GUEST, 2003)?;
        let sid = sign_on(port, "QSECOFR", SECOFR_PASSWORD)?;
        // Run SQL: the schema, the table, a row, the row read back.
        let page = run_sql(port, &sid, SCRIPT)?;
        want(&page, 200, &["SQL7956", "1 rows were inserted into ITEMS in AFW155.", ">BOLT<", ">0.25<", "4: SELECT"], "Run SQL's results")?;
        if page.body.contains("class=\"error\"") {
            return Err(format!("Run SQL reported an error:\n{}", page.brief()));
        }
        // Tables, the table's rows, Insert record.
        let tables = afw(port, "GET", "/db/tables?sp=AFW%25&s=AFW155&t=All", Some(&sid), None)?;
        want(&tables, 200, &[">ITEMS</a>"], "Tables")?;
        let view = afw(port, "GET", "/db/AFW155/ITEMS", Some(&sid), None)?;
        want(&view, 200, &["<h1>View: AFW155.ITEMS</h1>", ">BOLT<", "1 entry"], "the table's rows")?;
        let t = token(port, "/db/AFW155/ITEMS/insert", &sid)?;
        let form = format!("csrf={}&c1=2&c2=NUT&c3=0.10&insert=Insert", enc(&t));
        let r = afw(port, "POST", "/db/AFW155/ITEMS/insert", Some(&sid), Some(&form))?;
        want(&r, 303, &[], "Insert record")?;
        let view = afw(port, "GET", "/db/AFW155/ITEMS", Some(&sid), None)?;
        want(&view, 200, &["SQL7956", ">NUT<", ">0.10<", "2 entries"], "the inserted row")?;
        // Copy data to table: a CSV file of two rows.
        let t = token(port, "/db/copy/to?s=AFW155&t=ITEMS", &sid)?;
        let fields = [("csrf", t.as_str()), ("fmt", "CSV"), ("hdr", "on"), ("cp", "1208 (UTF-8)"), ("s", "AFW155"), ("t", "ITEMS"), ("m", "append"), ("e", "stop")];
        let read = multipart(port, "/db/copy/to", &sid, &fields, Some(("f", "items.csv", CSV.as_bytes())))?;
        want(&read, 200, &["Read from the first 50 rows of items.csv", "<option selected>PRICE</option>"], "the file's mapping")?;
        let t = csrf_in(&read.body).ok_or("no form field on Copy data to table")?;
        let go = multipart(port, "/db/copy/to", &sid, &[("csrf", t.as_str()), ("copy", "Copy")], None)?;
        let at = location(&go).ok_or_else(|| format!("Copy did not start:\n{}", go.brief()))?;
        let done = finished(port, &at, &sid)?;
        want(&done, 200, &["QAD7030", "2 rows in", "XWA0030"], "the copy to the table")?;
        // Copy data from table: all four rows as CSV.
        let t = token(port, "/db/copy/from?s=AFW155&t=ITEMS", &sid)?;
        let form = format!(
            "csrf={}&s=AFW155&t=ITEMS&q=&fmt=CSV&hdr=on&dt={}&cp={}&le=LF&copy={}",
            enc(&t),
            enc("ISO (YYYY-MM-DD)"),
            enc("UTF-8 (1208)"),
            enc("Copy (download)")
        );
        let go = afw(port, "POST", "/db/copy/from", Some(&sid), Some(&form))?;
        let at = location(&go).ok_or_else(|| format!("the copy from the table did not start:\n{}", go.brief()))?;
        let done = finished(port, &at, &sid)?;
        want(&done, 200, &["QAD7030", "4 rows in"], "the copy from the table")?;
        let file = afw(port, "GET", &format!("{at}&file=1"), Some(&sid), None)?;
        want(&file, 200, &["ID,NAME,PRICE\n", "1,BOLT,0.25\n", "2,NUT,0.10\n", "4,SCREW,1234567.89\n"], "the file copied out")?;
        let off = afw(port, "GET", "/signoff", Some(&sid), None)?;
        want(&off, 303, &[], "sign-off")?;
        // Another profile, without authority to the table: the
        // database's refusal, as that profile.
        s.command("RVKOBJAUT OBJ(AFW155/ITEMS) OBJTYPE(*FILE) USER(*PUBLIC) AUT(*ALL)", CMD)?;
        let usid = sign_on(port, USER, PASSWORD)?;
        let page = run_sql(port, &usid, "SELECT * FROM AFW155.ITEMS")?;
        want(&page, 200, &["SQL0551"], "Run SQL as a profile without authority")?;
        if page.body.contains(">BOLT<") {
            return Err(format!("{USER} read the table:\n{}", page.brief()));
        }
        afw(port, "GET", "/signoff", Some(&usid), None)?;
        let ended = s.command("ENDTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)", CMD)?;
        if !ended.contains("TCP1A0A") {
            return Err(format!("ENDTCPSVR ACCESSWEB:\n{ended}"));
        }
        s.command("DLTLIB LIB(AFW155)", CMD)?;
        s.command(&format!("DLTUSRPRF USRPRF({USER})"), CMD)?;
        Ok(format!(
            "over 2003 (host {port}): Run SQL made AFW155.ITEMS and read its row (SQL7956), Tables listed it, its page showed the row, Insert record added one, Copy data to table copied a CSV of two rows in (QAD7030), Copy data from table copied all four out with exact decimals; {USER} without authority got SQL0551"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_locations() {
        assert_eq!(csrf_in("<input type=\"hidden\" name=\"csrf\" value=\"ab\">").as_deref(), Some("ab"));
        let r = Reply {
            status: 303,
            head: "HTTP/1.1 303 See Other\r\nLocation: /qs400/accessweb/db/copy/to?job=1".into(),
            body: String::new(),
        };
        assert_eq!(location(&r).as_deref(), Some("/db/copy/to?job=1"));
    }
}
