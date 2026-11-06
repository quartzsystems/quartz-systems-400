// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Checking a signed image against a db certificate, the way the
//! firmware does: the image digest in the indirect data, the message
//! digest attribute over it, and the RSA signature over the attributes
//! with the certificate's key.

use der::{Decode, Encode};
use graviola::signing::rsa::VerifyingKey;
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use crate::der::{self as d, SET};
use crate::pe::{digest, embed};
use crate::sign::{MESSAGE_DIGEST, SIGNED_DATA, SPC_INDIRECT_DATA};
use crate::Error;

fn bad(what: &str) -> Error {
    Error::Signature(what.to_string())
}

/// Check `image`'s signature against the DER certificate `cert`.
pub fn verify(image: &[u8], cert: &[u8]) -> Result<(), Error> {
    let blob = embed::signature(image)?;
    let (ci, _) = d::read(blob)?;
    let ci = d::items(ci.content)?;
    if ci.first().map(|t| t.whole) != Some(&d::oid(SIGNED_DATA)[..]) {
        return Err(bad("not a PKCS#7 SignedData"));
    }
    let explicit = ci.get(1).ok_or_else(|| bad("no SignedData"))?;
    let (sd, _) = d::read(explicit.content)?;
    let sd = d::items(sd.content)?;
    // version, digestAlgorithms, contentInfo, [0] certificates, signerInfos
    let content = sd.get(2).ok_or_else(|| bad("no content"))?;
    let content = d::items(content.content)?;
    if content.first().map(|t| t.whole) != Some(&d::oid(SPC_INDIRECT_DATA)[..]) {
        return Err(bad("the content is not SpcIndirectDataContent"));
    }
    let (indirect, _) = d::read(content.get(1).ok_or_else(|| bad("no indirect data"))?.content)?;
    let parts = d::items(indirect.content)?;
    let digest_info = d::items(parts.get(1).ok_or_else(|| bad("no digest"))?.content)?;
    let want = digest::sha256(image)?;
    if digest_info.get(1).map(|t| t.content) != Some(&want[..]) {
        return Err(bad("the image digest does not match the signed one"));
    }
    let signers = d::items(sd.last().ok_or_else(|| bad("no SignerInfo"))?.content)?;
    let signer = d::items(signers.first().ok_or_else(|| bad("no SignerInfo"))?.content)?;
    let attrs = signer.iter().find(|t| t.tag == 0xA0).ok_or_else(|| bad("no signed attributes"))?;
    let md: [u8; 32] = Sha256::digest(indirect.content).into();
    let md_ok = d::items(attrs.content)?.iter().any(|a| {
        d::items(a.content).is_ok_and(|v| {
            v.first().map(|t| t.whole) == Some(&d::oid(MESSAGE_DIGEST)[..])
                && v.get(1).and_then(|s| d::read(s.content).ok()).is_some_and(|(o, _)| o.content == md)
        })
    });
    if !md_ok {
        return Err(bad("the message digest attribute does not match"));
    }
    let mut covered = attrs.whole.to_vec();
    covered[0] = SET;
    let sig = signer.last().ok_or_else(|| bad("no signature"))?.content;
    let c = Certificate::from_der(cert).map_err(|e| Error::Key(e.to_string()))?;
    let spki = c.tbs_certificate().subject_public_key_info();
    let pkcs1 = spki.subject_public_key.raw_bytes();
    let key = VerifyingKey::from_pkcs1_der(pkcs1).map_err(|e| Error::Key(format!("{e:?}")))?;
    key.verify_pkcs1_sha256(sig, &covered)
        .map_err(|_| bad("the RSA signature does not verify with the db certificate"))?;
    // The signer is the certificate given.
    let serial = c.tbs_certificate().serial_number().to_der().map_err(|e| Error::Key(e.to_string()))?;
    let ias = signer.get(1).ok_or_else(|| bad("no issuer and serial"))?;
    if !d::items(ias.content)?.iter().any(|t| t.whole == serial.as_slice()) {
        return Err(bad("the signer is not the db certificate"));
    }
    Ok(())
}
