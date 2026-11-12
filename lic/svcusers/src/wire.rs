// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The process-message encoding of a privilege set and of a refusal,
//! for a caller whose service tools ask another process (feature
//! `wire`).

use ipc::{Dec, Wire, WireError};

use crate::{Privilege, SignOnError};

impl Wire for Privilege {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(Self(u64::get(d)?))
    }
}

impl Wire for SignOnError {
    fn put(&self, out: &mut Vec<u8>) {
        self.code().put(out);
        match self {
            Self::BadPassword(why) => why.put(out),
            _ => String::new().put(out),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let code = u8::get(d)?;
        let why = String::get(d)?;
        Ok(Self::from_code(code, why))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip() {
        let e = SignOnError::BadPassword("empty".into());
        assert_eq!(ipc::decode::<SignOnError>(&ipc::encode(&e)).unwrap(), e);
        let e = SignOnError::Disabled;
        assert_eq!(ipc::decode::<SignOnError>(&ipc::encode(&e)).unwrap(), e);
        let p = Privilege::LOGS.with(Privilege::HARDWARE);
        assert_eq!(ipc::decode::<Privilege>(&ipc::encode(&p)).unwrap(), p);
    }
}
