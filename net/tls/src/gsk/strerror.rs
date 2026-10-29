// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `gsk_strerror`: Quartz text for each return code.

#[allow(clippy::wildcard_imports)] // the header's names
use super::codes::*;

/// The text of a return code.
#[allow(clippy::too_many_lines)] // one arm per code
pub fn strerror(rc: Rc) -> &'static str {
    match rc {
        GSK_OK => "The operation completed.",
        GSK_INVALID_HANDLE => "The handle does not name an open environment or secure socket.",
        GSK_API_NOT_AVAILABLE => "This function is not available on this system.",
        GSK_INTERNAL_ERROR => "An internal error occurred in the secure sockets layer.",
        GSK_INSUFFICIENT_STORAGE => "Storage could not be obtained.",
        GSK_INVALID_STATE => "The handle is not in the state this operation needs.",
        GSK_KEY_LABEL_NOT_FOUND => "No certificate in the store has the label given.",
        GSK_CERTIFICATE_NOT_AVAILABLE => "No certificate is available for this request.",
        GSK_ERROR_CERT_VALIDATION => "The peer's certificate did not validate.",
        GSK_ERROR_CRYPTO => "A cryptographic operation failed.",
        GSK_ERROR_ASN => "Certificate data could not be decoded.",
        GSK_ERROR_LDAP => "A directory operation failed.",
        GSK_ERROR_UNKNOWN_ERROR => "An error with no closer description occurred.",
        GSK_INVALID_PARAMETER => "A parameter is not valid.",
        GSK_OPEN_CIPHER_ERROR => "No cipher could be opened for the environment.",
        GSK_KEYFILE_IO_ERROR => "The certificate store could not be read or written.",
        GSK_KEYFILE_INVALID_FORMAT => "The file is not a Quartz certificate store.",
        GSK_KEYFILE_DUPLICATE_KEY => "The certificate store holds the same key twice.",
        GSK_KEYFILE_DUPLICATE_LABEL => "The certificate store holds the same label twice.",
        GSK_BAD_FORMAT_OR_INVALID_PASSWORD => {
            "The store password is not correct, or the file is damaged."
        }
        GSK_KEYFILE_CERT_EXPIRED => "The certificate selected from the store has expired.",
        GSK_ERROR_LOAD_GSKLIB => "The secure sockets support could not be loaded.",
        GSK_PENDING_CLOSE_ERROR => "Secure sockets are still open on this environment.",
        GSK_NO_KEYFILE_PASSWORD => "No store password was given and no stash exists.",
        GSK_KEYRING_OPEN_ERROR => "The certificate store could not be opened.",
        GSK_RSA_TEMP_KEY_PAIR => "A temporary RSA key pair could not be made.",
        GSK_ERROR_LDAP_NO_SUCH_OBJECT => "The directory entry does not exist.",
        GSK_ERROR_LDAP_INVALID_CREDENTIALS => "The directory refused the credentials.",
        GSK_ERROR_BAD_INDEX => "The index is out of range.",
        GSK_ERROR_FIPS_NOT_SUPPORTED => "FIPS mode is not supported.",
        GSK_CLOSE_FAILED => "The environment could not be closed.",
        GSK_ERROR_BAD_DATE => "The peer's certificate is outside its validity period.",
        GSK_ERROR_NO_CIPHERS => "The two sides share no protocol version or cipher suite.",
        GSK_ERROR_NO_CERTIFICATE => "No certificate is assigned for this session.",
        GSK_ERROR_BAD_CERTIFICATE => "The certificate is not valid.",
        GSK_ERROR_UNSUPPORTED_CERTIFICATE_TYPE => "The certificate type is not supported.",
        GSK_ERROR_IO => "The transport failed during a secure operation.",
        GSK_ERROR_BAD_KEYFILE_LABEL => "The certificate label is not valid.",
        GSK_ERROR_BAD_KEYFILE_PASSWORD => "The store password is not valid.",
        GSK_ERROR_BAD_KEY_LEN_FOR_EXPORT => "The key length is not allowed for export.",
        GSK_ERROR_BAD_MESSAGE => "The peer sent a message that is not valid.",
        GSK_ERROR_BAD_MAC => "A record failed its integrity check.",
        GSK_ERROR_UNSUPPORTED => "The operation is not supported.",
        GSK_ERROR_BAD_CERT_SIG => "The certificate's signature is not valid.",
        GSK_ERROR_BAD_CERT => "The certificate is damaged.",
        GSK_ERROR_BAD_PEER => "The peer's identity could not be accepted.",
        GSK_ERROR_PERMISSION_DENIED => "The operation is not permitted.",
        GSK_ERROR_SELF_SIGNED => "The peer's certificate is self-signed and not trusted.",
        GSK_ERROR_NO_READ_FUNCTION => "No read function is set.",
        GSK_ERROR_NO_WRITE_FUNCTION => "No write function is set.",
        GSK_ERROR_SOCKET_CLOSED => "The peer closed the connection.",
        GSK_ERROR_BAD_V2_CIPHER => "The SSL Version 2 cipher specification is not valid.",
        GSK_ERROR_BAD_V3_CIPHER => "The cipher specification is not valid.",
        GSK_ERROR_BAD_SEC_TYPE => "The security type is not valid.",
        GSK_ERROR_BAD_SEC_TYPE_COMBINATION => "The security types cannot be combined.",
        GSK_ERROR_HANDLE_CREATION_FAILED => "A handle could not be created.",
        GSK_ERROR_INITIALIZATION_FAILED => "Initialization failed.",
        GSK_ERROR_LDAP_NOT_AVAILABLE => "The directory is not available.",
        GSK_ERROR_NO_PRIVATE_KEY => "The certificate has no private key.",
        GSK_ERROR_INCORRECT_SESSION_TYPE => "The session type does not allow this operation.",
        GSK_ERROR_BAD_TLSV11_CIPHER => "The TLS Version 1.1 cipher specification is not valid.",
        GSK_ERROR_EMPTY_DATA_SEGMENT => "An empty data segment was received.",
        GSK_ERROR_NO_RI_INDICATION => "The peer does not support secure renegotiation.",
        GSK_ERROR_SEQNUM_EXHAUSTED => "The record sequence numbers are exhausted.",
        GSK_ERROR_NO_APPLICATION_PROTOCOL => "No application protocol was agreed.",
        GSK_INVALID_BUFFER_SIZE => "The buffer length is not valid.",
        GSK_WOULD_BLOCK => "The operation would block.",
        GSK_ERROR_SSL_CLOSED => "The secure session has been closed.",
        GSK_ERROR_NOT_SSLV3 => "The operation needs a session that can renegotiate.",
        GSK_MISC_INVALID_ID => "The miscellaneous operation ID is not valid.",
        GSK_ATTRIBUTE_INVALID_ID => "The attribute ID is not valid for this operation.",
        GSK_ATTRIBUTE_INVALID_LENGTH => "The attribute length is not valid.",
        GSK_ATTRIBUTE_INVALID_ENUMERATION => {
            "The enumeration value is not valid for the attribute."
        }
        GSK_ATTRIBUTE_INVALID_SID_CACHE => "The session cache setting is not valid.",
        GSK_ATTRIBUTE_INVALID_NUMERIC_VALUE => "The numeric value is not valid for the attribute.",
        GSK_CONFLICTING_VALIDATION_SETTING => "The validation settings conflict.",
        GSK_AES_UNSUPPORTED => "AES is not supported.",
        GSK_PEERID_LENGTH_ERROR => "The peer ID length is not valid.",
        GSK_CONFLICTING_ATTRIBUTE_SETTING => "The attribute settings conflict.",
        GSK_IBMI_ERROR_NOT_TRUSTED_ROOT => {
            "The peer's certificate chains to no trusted certificate authority."
        }
        GSK_IBMI_ERROR_PASSWORD_EXPIRED => "The store password has expired.",
        GSK_IBMI_ERROR_NOT_REGISTERED => "The application ID is not registered.",
        GSK_IBMI_ERROR_NO_ACCESS => "The job is not authorized to the certificate store.",
        GSK_IBMI_ERROR_CLOSED => "The environment or socket is closed.",
        GSK_IBMI_ERROR_NO_CERTIFICATE_AUTHORITIES => {
            "The application trusts no certificate authority."
        }
        GSK_IBMI_ERROR_NO_INITIALIZE => "The environment has not been initialized.",
        GSK_IBMI_ERROR_ALREADY_SECURE => "The socket is already secure.",
        GSK_IBMI_ERROR_NOT_TCP => "The socket is not a TCP socket.",
        GSK_IBMI_ERROR_INVALID_POINTER => "A pointer parameter is not valid.",
        GSK_IBMI_ERROR_TIMED_OUT => "The operation timed out.",
        GSK_IBMI_ASYNCHRONOUS_RECV => "The receive will complete asynchronously.",
        GSK_IBMI_ASYNCHRONOUS_SEND => "The send will complete asynchronously.",
        GSK_IBMI_ERROR_INVALID_OVERLAPPEDIO_T => "The overlapped I/O structure is not valid.",
        GSK_IBMI_ERROR_INVALID_IOCOMPLETIONPORT => "The I/O completion port is not valid.",
        GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR => "The socket descriptor is not valid.",
        GSK_IBMI_ERROR_CERTIFICATE_REVOKED => "The peer's certificate has been revoked.",
        GSK_IBMI_ERROR_CRL_INVALID => "The certificate revocation list is not valid.",
        GSK_IBMI_ASYNCHRONOUS_SOC_INIT => "The handshake will complete asynchronously.",
        _ => "The return code is not one the secure sockets layer knows.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_code_has_its_own_text() {
        let unknown = strerror(-12345);
        for rc in [
            0, 1, 5, 106, 201, 402, 403, 420, 502, 503, 701, 703, 705, 6000, 6002, 6016,
        ] {
            assert_ne!(strerror(rc), unknown, "{rc}");
        }
    }
}
