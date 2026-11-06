// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pesign`: `keygen DIR [SUBJECT]` makes a db key pair (PKCS#8 PEM
//! key, DER certificate, signature list); `sign IN OUT` signs with
//! `QS400_SB_KEY`/`QS400_SB_CERT` or the development pair; `verify
//! FILE CERT` checks a signed image; `esl CERT OUT` writes a signature
//! list; `show` prints the key in use.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use pesign::DbKey;
use pesign::keys::{esl, generate};

/// 2026-01-01T00:00:00Z, the build's fixed epoch.
const NOT_BEFORE: u64 = 1_767_225_600;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["keygen", dir, rest @ ..] => {
            let subject = rest.first().copied().unwrap_or("CN=QS/400 Secure Boot db,O=Quartz Systems");
            let k = generate::new_pair(&generate::Spec { subject, not_before: NOT_BEFORE, years: 30 })?;
            let dir = Path::new(dir);
            fs::create_dir_all(dir)?;
            let pk = k.key.to_pkcs8_der().map_err(|e| anyhow::anyhow!("{e}"))?;
            fs::write(dir.join("qs400-db.key"), certs::pem::encode("PRIVATE KEY", &pk))?;
            fs::write(dir.join("qs400-db.cer"), &k.cert)?;
            fs::write(dir.join("qs400-db.esl"), esl::list(&k.cert))?;
            println!("db key in {}: SHA-256 {}", dir.display(), k.fingerprint());
        }
        ["sign", input, output] => {
            let k = DbKey::from_env()?;
            let image = fs::read(input).with_context(|| input.to_string())?;
            fs::write(output, pesign::sign(&image, &k)?)?;
            println!("{output} signed with {}{}", k.fingerprint(), if k.development { " (DEVELOPMENT key)" } else { "" });
        }
        ["verify", file, cert] => {
            let cert = fs::read(cert)?;
            pesign::verify(&fs::read(file)?, &cert)?;
            println!("{file}: signature good, signer {}", pesign::keys::fingerprint(&cert));
        }
        ["esl", cert, out] => {
            fs::write(out, esl::list(&fs::read(cert)?))?;
        }
        ["show"] => {
            let k = DbKey::from_env()?;
            println!("{}{}", k.fingerprint(), if k.development { " (DEVELOPMENT key)" } else { "" });
        }
        _ => bail!("usage: pesign keygen DIR [SUBJECT] | sign IN OUT | verify FILE CERT | esl CERT OUT | show"),
    }
    Ok(())
}
