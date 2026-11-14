// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Responses the job writes itself: a request's failure when the
//! service did not answer it, and `StartTLS` refused.

use ldap::ResultCode;
use ldap::ext;
use ldap::msg::{LdapMessage, LdapResult, ProtocolOp};

/// The response to request `req` with `code` and `message`; `None` for
/// a request that has none (unbind, abandon).
pub fn failure(req: &LdapMessage, code: ResultCode, message: &str) -> Option<Vec<u8>> {
    let r = LdapResult::with_message(code, message);
    let op = match &req.op {
        ProtocolOp::BindRequest { .. } => ProtocolOp::BindResponse {
            result: r,
            server_sasl_creds: None,
        },
        ProtocolOp::SearchRequest(_) => ProtocolOp::SearchResultDone(r),
        ProtocolOp::ModifyRequest { .. } => ProtocolOp::ModifyResponse(r),
        ProtocolOp::AddRequest { .. } => ProtocolOp::AddResponse(r),
        ProtocolOp::DelRequest(_) => ProtocolOp::DelResponse(r),
        ProtocolOp::ModDnRequest { .. } => ProtocolOp::ModDnResponse(r),
        ProtocolOp::CompareRequest { .. } => ProtocolOp::CompareResponse(r),
        ProtocolOp::ExtendedRequest { name, .. } => ProtocolOp::ExtendedResponse {
            result: r,
            name: (name == ext::START_TLS).then(|| name.clone()),
            value: None,
        },
        _ => return None,
    };
    Some(LdapMessage::new(req.id, op).encode())
}

/// A persistent search the service ended.
pub fn search_ended(id: i32) -> Vec<u8> {
    LdapMessage::new(
        id,
        ProtocolOp::SearchResultDone(LdapResult::with_message(
            ResultCode::Other(80),
            "The persistent search ended.",
        )),
    )
    .encode()
}

/// The notice of disconnection.
pub fn notice(code: ResultCode, message: &str) -> Vec<u8> {
    ext::notice_of_disconnection(code, message).encode()
}
