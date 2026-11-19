// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PKCS#12 structures (RFC 7292) and the PKCS#7 content wrappers
//! they sit in.

use der::asn1::{ObjectIdentifier, OctetString, SetOfVec};
use der::{Any, Sequence, ValueOrd};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;

/// id-data.
pub const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
/// id-encryptedData.
pub const ID_ENCRYPTED_DATA: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.6");
/// keyBag.
pub const KEY_BAG: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.1");
/// pkcs8ShroudedKeyBag.
pub const SHROUDED_KEY_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.2");
/// certBag.
pub const CERT_BAG: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.3");
/// x509Certificate (inside a certBag).
pub const X509_CERTIFICATE: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.22.1");
/// friendlyName.
pub const FRIENDLY_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.20");
/// localKeyId.
pub const LOCAL_KEY_ID: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.21");
/// id-sha1.
pub const SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
/// id-sha256.
pub const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// PFX.
#[derive(Clone, Debug, Sequence)]
pub struct Pfx {
    /// Always 3.
    pub version: u8,
    /// The authenticated safe, a `Data` content.
    pub auth_safe: ContentInfo,
    /// Integrity: the password MAC.
    pub mac_data: Option<MacData>,
}

/// PKCS#7 `ContentInfo`.
#[derive(Clone, Debug, Sequence, ValueOrd)]
pub struct ContentInfo {
    /// `id-data` or `id-encryptedData`.
    pub content_type: ObjectIdentifier,
    /// An OCTET STRING for data, an `EncryptedData` otherwise.
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    pub content: Any,
}

/// PKCS#7 `EncryptedData`.
#[derive(Clone, Debug, Sequence)]
pub struct EncryptedData {
    /// Always 0.
    pub version: u8,
    /// What is encrypted and how.
    pub encrypted_content_info: EncryptedContentInfo,
}

/// PKCS#7 `EncryptedContentInfo`.
#[derive(Clone, Debug, Sequence)]
pub struct EncryptedContentInfo {
    /// `id-data`.
    pub content_type: ObjectIdentifier,
    /// The PBE scheme.
    pub content_encryption_algorithm: AlgorithmIdentifierOwned,
    /// The ciphertext.
    #[asn1(context_specific = "0", tag_mode = "IMPLICIT", optional = "true")]
    pub encrypted_content: Option<OctetString>,
}

/// `MacData`.
#[derive(Clone, Debug, Sequence)]
pub struct MacData {
    /// The MAC and its hash.
    pub mac: DigestInfo,
    /// The MAC salt.
    pub mac_salt: OctetString,
    /// Iterations (1 when absent).
    #[asn1(default = "default_iterations")]
    pub iterations: u32,
}

fn default_iterations() -> u32 {
    1
}

/// `DigestInfo`.
#[derive(Clone, Debug, Sequence)]
pub struct DigestInfo {
    /// The hash.
    pub algorithm: AlgorithmIdentifierOwned,
    /// The value.
    pub digest: OctetString,
}

/// `SafeBag`.
#[derive(Clone, Debug, Sequence, ValueOrd)]
pub struct SafeBag {
    /// The bag type.
    pub bag_id: ObjectIdentifier,
    /// The bag's content.
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    pub bag_value: Any,
    /// `friendlyName`, `localKeyId`.
    pub bag_attributes: Option<SetOfVec<Attribute>>,
}

/// `CertBag`.
#[derive(Clone, Debug, Sequence)]
pub struct CertBag {
    /// `x509Certificate`.
    pub cert_id: ObjectIdentifier,
    /// An OCTET STRING holding the DER certificate.
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    pub cert_value: Any,
}

/// `EncryptedPrivateKeyInfo` (PKCS#8).
#[derive(Clone, Debug, Sequence)]
pub struct EncryptedPrivateKeyInfo {
    /// The PBE scheme.
    pub encryption_algorithm: AlgorithmIdentifierOwned,
    /// The encrypted `PrivateKeyInfo`.
    pub encrypted_data: OctetString,
}
