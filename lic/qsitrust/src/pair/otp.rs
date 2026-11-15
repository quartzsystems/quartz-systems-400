// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The one-time pairing password: sixteen characters of Crockford's
//! base32 (80 bits) in four groups, read back however the operator
//! types them.

use crate::Error;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A new password, `XXXX-XXXX-XXXX-XXXX`.
///
/// # Errors
/// [`Error::Random`] when there is no random source.
pub fn one_time_password() -> Result<String, Error> {
    let mut bytes = [0u8; 16];
    graviola::random::fill(&mut bytes).map_err(|_| Error::Random)?;
    let mut out = String::with_capacity(19);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(char::from(ALPHABET[usize::from(b & 31)]));
    }
    Ok(out)
}

/// The password as both ends compare it: upper case, without spaces or
/// dashes, the letters Crockford's base32 reads as digits read so.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_and_typed() {
        let p = one_time_password().unwrap();
        assert_eq!(p.len(), 19);
        assert_eq!(p.matches('-').count(), 3);
        assert_ne!(p, one_time_password().unwrap());
        assert_eq!(normalize(&p.to_lowercase()), normalize(&p));
        assert_eq!(normalize("ab0o-1il x"), "AB00111X");
    }
}
