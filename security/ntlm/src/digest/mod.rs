// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The digests NTLM needs: MD4, MD5 and HMAC-MD5.

mod hmac;
mod md4;
mod md5;
mod pad;

pub use hmac::hmac_md5;
pub use md4::md4;
pub use md5::md5;
