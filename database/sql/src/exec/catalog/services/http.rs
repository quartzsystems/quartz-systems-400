// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.HTTP_GET`, `HTTP_POST`, `HTTP_PUT`, `HTTP_PATCH`,
//! `HTTP_DELETE`, their `_BLOB` forms and the `_VERBOSE` table
//! functions. The request goes through the host's HTTP client (the
//! storage service's, over `net/httpc` with TLS); the options are
//! IBM's JSON object, of which `header`, `headers`, `basicAuth`,
//! `sslTolerate`, `ioTimeout`, `connectTimeout` and `redirect` are
//! honoured and any other is refused (SQL0443).

use std::fmt::Write as _;

use store::txn::Txn;

use super::super::super::SqlContext;
use super::super::super::json::Json;
use super::super::super::restype::{Kind, SqlType};
use super::host::{HttpRequest, HttpResponse};
use super::{Args, Out};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The method and whether the body is binary, of a scalar name.
fn method_of(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "HTTP_GET" => ("GET", false),
        "HTTP_GET_BLOB" => ("GET", true),
        "HTTP_POST" => ("POST", false),
        "HTTP_POST_BLOB" => ("POST", true),
        "HTTP_PUT" => ("PUT", false),
        "HTTP_PUT_BLOB" => ("PUT", true),
        "HTTP_PATCH" => ("PATCH", false),
        "HTTP_PATCH_BLOB" => ("PATCH", true),
        "HTTP_DELETE" => ("DELETE", false),
        "HTTP_DELETE_BLOB" => ("DELETE", true),
        _ => return None,
    })
}

fn failed(what: impl Into<String>) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what: what.into() }
}

/// The request of `url`, `body` and the options' JSON.
fn request(method: &str, url: &str, body: Vec<u8>, options: Option<&str>) -> Result<HttpRequest> {
    let mut req = HttpRequest { method: method.into(), url: url.trim().into(), body, redirects: 10, ..HttpRequest::default() };
    let Some(text) = options.filter(|t| !t.trim().is_empty()) else {
        return Ok(req);
    };
    let Ok(Json::Object(members)) = super::super::super::json::value::parse(text) else {
        return Err(failed("the options are not a JSON object"));
    };
    let text_of = |v: &Json| match v {
        Json::Str(s) => s.clone(),
        Json::Num(n) => n.clone(),
        Json::Bool(b) => b.to_string(),
        other => other.text(),
    };
    for (k, v) in members {
        match k.as_str() {
            "header" => {
                let t = text_of(&v);
                let (n, val) = t.split_once(',').ok_or_else(|| failed("a header option is not name,value"))?;
                req.headers.push((n.trim().into(), val.trim().into()));
            }
            "headers" => {
                let Json::Object(h) = v else { return Err(failed("headers is not a JSON object")) };
                for (n, val) in h {
                    req.headers.push((n, text_of(&val)));
                }
            }
            "basicAuth" => {
                let t = text_of(&v);
                let (u, p) = t.split_once(',').ok_or_else(|| failed("basicAuth is not user,password"))?;
                let token = super::super::super::func::services::base64_encode(format!("{u}:{p}").as_bytes());
                req.headers.push(("Authorization".into(), format!("Basic {token}")));
            }
            "sslTolerate" => req.tolerate = text_of(&v).eq_ignore_ascii_case("true"),
            "ioTimeout" | "connectTimeout" => {
                // Milliseconds; the client takes whole seconds.
                let ms: u64 = text_of(&v).parse().map_err(|_| failed(format!("{k} is not a number")))?;
                let secs = u32::try_from(ms.div_ceil(1000)).unwrap_or(u32::MAX);
                req.timeout = if req.timeout == 0 { secs } else { req.timeout.max(secs) };
            }
            "redirect" => req.redirects = text_of(&v).parse().map_err(|_| failed("redirect is not a number"))?,
            other => return Err(failed(format!("option {other} is not supported"))),
        }
    }
    Ok(req)
}

fn send(ctx: &SqlContext, req: &HttpRequest) -> Result<HttpResponse> {
    let host = ctx.host.as_deref().ok_or_else(|| super::unreachable_here("the HTTP client"))?;
    host.http(req).ok_or_else(|| super::unreachable_here("the HTTP client"))?.map_err(failed)
}

/// The body argument: text as UTF-8, binary as it is.
fn body_of(v: &Value) -> Vec<u8> {
    match v {
        Value::Null => Vec::new(),
        Value::Bin(b) => b.clone(),
        v => v.as_text().into_bytes(),
    }
}

/// A scalar HTTP function, `None` when `name` is not one.
pub(crate) fn scalar(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    let bare = name.strip_prefix("QSYS2.").unwrap_or(name);
    let (method, binary) = method_of(bare)?;
    Some((|| {
        let with_body = matches!(method, "POST" | "PUT" | "PATCH");
        let need = if with_body { 2 } else { 1 };
        if args.len() < need || args.len() > need + 1 {
            return Err(SqlError::Coded { code: -440, state: "42884", what: format!("QSYS2.{bare} with {} arguments", args.len()) });
        }
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let body = if with_body { body_of(&args[1]) } else { Vec::new() };
        let options = args.get(need).filter(|v| !v.is_null()).map(Value::as_text);
        let req = request(method, &args[0].as_text(), body, options.as_deref())?;
        let res = send(ctx, &req)?;
        if !(200..300).contains(&res.status) {
            return Err(failed(format!("the server answered HTTP {}", res.status)));
        }
        Ok(if binary { Value::Bin(res.body) } else { Value::Str(String::from_utf8_lossy(&res.body).into_owned()) })
    })())
}

/// A scalar HTTP function's result type.
pub(crate) fn result_type(name: &str) -> Option<SqlType> {
    let (_, binary) = method_of(name)?;
    Some(if binary {
        SqlType::new(Kind::Blob, 2_147_483_647, 0, true)
    } else {
        let mut t = SqlType::new(Kind::Clob, 2_147_483_647, 0, true);
        t.spec.ccsid = Some(1208);
        t
    })
}

/// The response headers as IBM's JSON object: the status first.
fn header_json(res: &HttpResponse) -> String {
    let mut out = String::from("{");
    let _ = write!(out, "\"HTTP_STATUS_CODE\":{}", res.status);
    for (n, v) in &res.headers {
        out.push(',');
        super::super::super::json::value::quote(n, &mut out);
        out.push(':');
        super::super::super::json::value::quote(v, &mut out);
    }
    out.push('}');
    out
}

/// A `_VERBOSE` table function: one row, the body and the headers.
pub fn verbose(_txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out, method: &str, binary: bool) -> Result<()> {
    let Some(url) = args.text("URL") else {
        return Ok(());
    };
    let body = body_of(args.get("REQUEST_MESSAGE"));
    let req = request(method, &url, body, args.text("OPTIONS").as_deref())?;
    let res = send(ctx, &req)?;
    out.row();
    if binary {
        out.set("RESPONSE_MESSAGE", Value::Bin(res.body.clone()));
    } else {
        out.text("RESPONSE_MESSAGE", String::from_utf8_lossy(&res.body).into_owned());
    }
    out.text("RESPONSE_HTTP_HEADER", header_json(&res));
    Ok(())
}

macro_rules! verbose_fn {
    ($($f:ident $m:literal $b:literal),* $(,)?) => {
        $(
            /// One `_VERBOSE` function.
            pub fn $f(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
                verbose(txn, ctx, args, out, $m, $b)
            }
        )*
    };
}

verbose_fn!(
    get_v "GET" false,
    get_bv "GET" true,
    post_v "POST" false,
    post_bv "POST" true,
    put_v "PUT" false,
    put_bv "PUT" true,
    patch_v "PATCH" false,
    patch_bv "PATCH" true,
    delete_v "DELETE" false,
    delete_bv "DELETE" true,
);
