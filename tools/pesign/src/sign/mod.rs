// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Authenticode signature: a PKCS#7 `SignedData` whose content is an
//! `SpcIndirectDataContent` holding the image digest, signed with
//! RSA PKCS#1 v1.5 over SHA-256 by the db key, the db certificate
//! included. RSA PKCS#1 v1.5 is deterministic: the same image and key
//! give the same bytes.

pub mod verify;

use sha2::{Digest, Sha256};

use crate::Error;
use crate::der::{self, BITS, OCTETS, SEQ, SET, alg, cons, int, oid, set_of, tlv};
use crate::keys::DbKey;
use crate::pe::{digest, embed};

/// `signedData`.
pub const SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
/// `SPC_INDIRECT_DATA_OBJID`.
pub const SPC_INDIRECT_DATA: &str = "1.3.6.1.4.1.311.2.1.4";
/// `SPC_PE_IMAGE_DATAOBJ`.
pub const SPC_PE_IMAGE_DATA: &str = "1.3.6.1.4.1.311.2.1.15";
/// `SPC_SP_OPUS_INFO_OBJID`.
pub const SPC_SP_OPUS_INFO: &str = "1.3.6.1.4.1.311.2.1.12";
/// `id-sha256`.
pub const SHA256: &str = "2.16.840.1.101.3.4.2.1";
/// `rsaEncryption`.
pub const RSA: &str = "1.2.840.113549.1.1.1";
/// `contentType`.
pub const CONTENT_TYPE: &str = "1.2.840.113549.1.9.3";
/// `messageDigest`.
pub const MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";

/// The `SpcIndirectDataContent` for an image digest.
#[must_use]
pub fn indirect_data(image_digest: &[u8; 32]) -> Vec<u8> {
    // SpcPeImageData: no flags, the file link "<<<Obsolete>>>" as a
    // BMPString, as every Authenticode signer writes it.
    let obsolete: Vec<u8> = "<<<Obsolete>>>"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    let link = tlv(0xA0, &tlv(0xA2, &tlv(0x80, &obsolete)));
    let pe_image = cons(SEQ, &[&tlv(BITS, &[0]), &link]);
    let data = cons(SEQ, &[&oid(SPC_PE_IMAGE_DATA), &pe_image]);
    let digest_info = cons(SEQ, &[&alg(SHA256), &tlv(OCTETS, image_digest)]);
    cons(SEQ, &[&data, &digest_info])
}

/// The signed attributes' members: content type, the digest of the
/// indirect data's content octets, and an empty `SpcSpOpusInfo`.
fn attributes(indirect: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let (t, _) = der::read(indirect)?;
    let md: [u8; 32] = Sha256::digest(t.content).into();
    Ok(vec![
        cons(
            SEQ,
            &[&oid(CONTENT_TYPE), &tlv(SET, &oid(SPC_INDIRECT_DATA))],
        ),
        cons(SEQ, &[&oid(MESSAGE_DIGEST), &tlv(SET, &tlv(OCTETS, &md))]),
        cons(SEQ, &[&oid(SPC_SP_OPUS_INFO), &tlv(SET, &tlv(SEQ, &[]))]),
    ])
}

/// The PKCS#7 `ContentInfo` signing `image_digest` with `key`.
pub fn signed_data(image_digest: &[u8; 32], key: &DbKey) -> Result<Vec<u8>, Error> {
    let indirect = indirect_data(image_digest);
    let attrs = attributes(&indirect)?;
    // The signature covers the attributes as a DER SET OF; the
    // SignerInfo carries the same bytes under [0] IMPLICIT.
    let as_set = set_of(attrs.clone(), SET);
    let sig = key
        .key
        .sign(&as_set)
        .map_err(|e| Error::Key(e.to_string()))?;
    let (issuer, serial) = key.issuer_and_serial()?;
    let signer = cons(
        SEQ,
        &[
            &int(1),
            &cons(SEQ, &[&issuer, &serial]),
            &alg(SHA256),
            &set_of(attrs, 0xA0),
            &alg(RSA),
            &tlv(OCTETS, &sig),
        ],
    );
    let content_info = cons(SEQ, &[&oid(SPC_INDIRECT_DATA), &tlv(0xA0, &indirect)]);
    let sd = cons(
        SEQ,
        &[
            &int(1),
            &tlv(SET, &alg(SHA256)),
            &content_info,
            &tlv(0xA0, &key.cert),
            &tlv(SET, &signer),
        ],
    );
    Ok(cons(SEQ, &[&oid(SIGNED_DATA), &tlv(0xA0, &sd)]))
}

/// `image` signed with `key`: any old signature dropped, the image
/// padded to 8 bytes, the certificate table appended.
pub fn sign(image: &[u8], key: &DbKey) -> Result<Vec<u8>, Error> {
    let mut out = embed::unsigned(image)?;
    let d = digest::sha256(&out)?;
    let sd = signed_data(&d, key)?;
    embed::attach(&mut out, &sd)?;
    verify::verify(&out, &key.cert)?;
    Ok(out)
}
