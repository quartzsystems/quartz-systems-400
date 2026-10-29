// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The adapter between [`PrivateKey`] and x509-cert's builders.

use der::asn1::BitString;
use spki::{
    AlgorithmIdentifierOwned, Document, DynSignatureAlgorithmIdentifier, EncodePublicKey,
    SignatureBitStringEncoding,
};

use crate::crypto::keys::PrivateKey;

/// A private key as the builders want it.
pub struct Signer<'a>(pub &'a PrivateKey);

/// The public half, as `SubjectPublicKeyInfo` DER.
#[derive(Clone)]
pub struct PublicKey(Vec<u8>);

impl EncodePublicKey for PublicKey {
    fn to_public_key_der(&self) -> spki::Result<Document> {
        Document::try_from(self.0.clone()).map_err(spki::Error::from)
    }
}

/// A DER signature.
pub struct DerSignature(Vec<u8>);

impl SignatureBitStringEncoding for DerSignature {
    fn to_bitstring(&self) -> der::Result<BitString> {
        BitString::from_bytes(&self.0)
    }
}

impl signature::Keypair for Signer<'_> {
    type VerifyingKey = PublicKey;

    fn verifying_key(&self) -> PublicKey {
        PublicKey(self.0.spki_der().unwrap_or_default())
    }
}

impl DynSignatureAlgorithmIdentifier for Signer<'_> {
    fn signature_algorithm_identifier(&self) -> spki::Result<AlgorithmIdentifierOwned> {
        Ok(self.0.signature_algorithm())
    }
}

impl signature::Signer<DerSignature> for Signer<'_> {
    fn try_sign(&self, msg: &[u8]) -> Result<DerSignature, signature::Error> {
        self.0
            .sign(msg)
            .map(DerSignature)
            .map_err(|_| signature::Error::new())
    }
}
