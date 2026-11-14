// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended operations: the OIDs QS/400 knows (docs/DIRSRV.md §7) and
//! their request and response values.

use crate::ber::{BerError, Reader, Writer, tag};
use crate::code::ResultCode;
use crate::msg::{LdapMessage, LdapResult, ProtocolOp};

/// `StartTLS` (RFC 4511 §4.14).
pub const START_TLS: &str = "1.3.6.1.4.1.1466.20037";
/// Who am I (RFC 4532).
pub const WHO_AM_I: &str = "1.3.6.1.4.1.4203.1.11.3";
/// Password modify (RFC 3062).
pub const PASSWORD_MODIFY: &str = "1.3.6.1.4.1.4203.1.11.1";
/// Notice of disconnection (RFC 4511 §4.4.1).
pub const NOTICE_OF_DISCONNECTION: &str = "1.3.6.1.4.1.1466.20036";

/// Why an extended operation's value does not decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtError {
    /// Not well-formed BER.
    #[error("bad extended operation value: {0}")]
    Ber(#[from] BerError),
    /// Well-formed but wrong.
    #[error("bad extended operation value: {0}")]
    Bad(&'static str),
}

/// The `StartTLS` request.
#[must_use]
pub fn start_tls() -> ProtocolOp {
    ProtocolOp::ExtendedRequest {
        name: START_TLS.into(),
        value: None,
    }
}

/// The who-am-I request.
#[must_use]
pub fn who_am_i() -> ProtocolOp {
    ProtocolOp::ExtendedRequest {
        name: WHO_AM_I.into(),
        value: None,
    }
}

/// The who-am-I response: the authorization identity (`dn:...`, or empty
/// for anonymous) is the value itself and the response has no name.
#[must_use]
pub fn who_am_i_response(result: LdapResult, authz_id: &str) -> ProtocolOp {
    ProtocolOp::ExtendedResponse {
        result,
        name: None,
        value: Some(authz_id.as_bytes().to_vec()),
    }
}

/// The authorization identity of a who-am-I response's value (none
/// means anonymous).
///
/// # Errors
///
/// [`ExtError::Bad`] if it is not UTF-8.
pub fn who_am_i_value(v: Option<&[u8]>) -> Result<String, ExtError> {
    String::from_utf8(v.unwrap_or_default().to_vec())
        .map_err(|_| ExtError::Bad("authorization identity is not UTF-8"))
}

/// The password modify request (RFC 3062).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PasswordModifyRequest {
    /// The user, if not the bound one.
    pub user: Option<String>,
    /// The old password.
    pub old: Option<Vec<u8>>,
    /// The new password; none asks the server to generate one.
    pub new: Option<Vec<u8>>,
}

impl PasswordModifyRequest {
    /// The request value.
    #[must_use]
    pub fn encode_value(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.constructed(tag::SEQUENCE, |w| {
            if let Some(u) = &self.user {
                w.string(tag::context(0), u);
            }
            if let Some(o) = &self.old {
                w.octets(tag::context(1), o);
            }
            if let Some(n) = &self.new {
                w.octets(tag::context(2), n);
            }
        });
        w.into_bytes()
    }

    /// The extended request.
    #[must_use]
    pub fn to_op(&self) -> ProtocolOp {
        ProtocolOp::ExtendedRequest {
            name: PASSWORD_MODIFY.into(),
            value: Some(self.encode_value()),
        }
    }

    /// Decodes a request value (none is a request with no fields).
    ///
    /// # Errors
    ///
    /// An [`ExtError`].
    pub fn decode_value(v: Option<&[u8]>) -> Result<Self, ExtError> {
        let Some(v) = v else {
            return Ok(Self::default());
        };
        let mut r = Reader::new(v);
        let mut s = r.sequence(tag::SEQUENCE)?;
        r.finish()?;
        let user = s
            .opt_octets(tag::context(0))?
            .map(|u| String::from_utf8(u.to_vec()).map_err(|_| ExtError::Bad("user is not UTF-8")))
            .transpose()?;
        let old = s.opt_octets(tag::context(1))?.map(<[u8]>::to_vec);
        let new = s.opt_octets(tag::context(2))?.map(<[u8]>::to_vec);
        s.finish()?;
        Ok(Self { user, old, new })
    }
}

/// The password modify response: the generated password, if any.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PasswordModifyResponse {
    /// The password the server generated.
    pub generated: Option<Vec<u8>>,
}

impl PasswordModifyResponse {
    /// The response value (none when nothing was generated).
    #[must_use]
    pub fn encode_value(&self) -> Option<Vec<u8>> {
        self.generated.as_ref().map(|g| {
            let mut w = Writer::new();
            w.constructed(tag::SEQUENCE, |w| w.octets(tag::context(0), g));
            w.into_bytes()
        })
    }

    /// Decodes a response value.
    ///
    /// # Errors
    ///
    /// An [`ExtError`].
    pub fn decode_value(v: Option<&[u8]>) -> Result<Self, ExtError> {
        let Some(v) = v else {
            return Ok(Self::default());
        };
        let mut r = Reader::new(v);
        let mut s = r.sequence(tag::SEQUENCE)?;
        r.finish()?;
        let generated = s.opt_octets(tag::context(0))?.map(<[u8]>::to_vec);
        s.finish()?;
        Ok(Self { generated })
    }
}

/// The notice of disconnection (RFC 4511 §4.4.1): an unsolicited
/// extended response, message ID 0, before the server closes.
#[must_use]
pub fn notice_of_disconnection(code: ResultCode, message: &str) -> LdapMessage {
    LdapMessage::new(
        0,
        ProtocolOp::ExtendedResponse {
            result: LdapResult::with_message(code, message),
            name: Some(NOTICE_OF_DISCONNECTION.into()),
            value: None,
        },
    )
}

/// The result of a notice of disconnection, if `m` is one.
#[must_use]
pub fn as_notice_of_disconnection(m: &LdapMessage) -> Option<&LdapResult> {
    match &m.op {
        ProtocolOp::ExtendedResponse {
            result,
            name: Some(n),
            ..
        } if m.id == 0 && n == NOTICE_OF_DISCONNECTION => Some(result),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PasswordModifyRequest, PasswordModifyResponse, as_notice_of_disconnection,
        notice_of_disconnection, start_tls, who_am_i_response, who_am_i_value,
    };
    use crate::code::ResultCode;
    use crate::msg::{LdapMessage, LdapResult, ProtocolOp};

    #[test]
    fn password_modify() {
        let r = PasswordModifyRequest {
            user: Some("u:joe".into()),
            old: None,
            new: Some(b"pw".to_vec()),
        };
        // SEQUENCE { [0] "u:joe", [2] "pw" }
        assert_eq!(
            r.encode_value(),
            [
                0x30, 0x0B, 0x80, 0x05, b'u', b':', b'j', b'o', b'e', 0x82, 0x02, b'p', b'w'
            ]
        );
        assert_eq!(
            PasswordModifyRequest::decode_value(Some(&r.encode_value())).unwrap(),
            r
        );
        assert_eq!(
            PasswordModifyRequest::decode_value(None).unwrap(),
            PasswordModifyRequest::default()
        );
        // Out of order: [2] before [0].
        assert!(
            PasswordModifyRequest::decode_value(Some(&[
                0x30, 0x06, 0x82, 0x01, b'p', 0x80, 0x01, b'u'
            ]))
            .is_err()
        );
        let g = PasswordModifyResponse {
            generated: Some(b"new".to_vec()),
        };
        assert_eq!(
            g.encode_value().unwrap(),
            [0x30, 0x05, 0x80, 0x03, b'n', b'e', b'w']
        );
        assert_eq!(
            PasswordModifyResponse::decode_value(g.encode_value().as_deref()).unwrap(),
            g
        );
        assert_eq!(PasswordModifyResponse::default().encode_value(), None);
    }

    #[test]
    fn notices_and_who_am_i() {
        let n = notice_of_disconnection(ResultCode::Unavailable, "going down");
        let back = LdapMessage::decode(&n.encode()).unwrap();
        assert_eq!(
            as_notice_of_disconnection(&back).unwrap().code,
            ResultCode::Unavailable
        );
        assert!(as_notice_of_disconnection(&LdapMessage::new(1, start_tls())).is_none());
        let ProtocolOp::ExtendedResponse { value, name, .. } =
            who_am_i_response(LdapResult::success(), "dn:cn=a")
        else {
            panic!()
        };
        assert!(name.is_none());
        assert_eq!(who_am_i_value(value.as_deref()).unwrap(), "dn:cn=a");
        assert_eq!(who_am_i_value(None).unwrap(), "");
    }
}
