// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One reference code and its explanation.

use super::{Family, Phase};

/// One reference code.
#[derive(Debug, PartialEq, Eq)]
pub struct Src {
    /// Eight-character code.
    pub code: &'static str,
    /// Short text posted with the code.
    pub text: &'static str,
    /// Phase.
    pub phase: Phase,
    /// What the code means.
    pub meaning: &'static str,
    /// Why it is posted, where written; `None` where not.
    pub cause: Option<&'static str>,
    /// What to do about it, where written; `None` where not.
    pub recovery: Option<&'static str>,
}

impl Src {
    /// A code with its meaning and no cause or recovery.
    pub(crate) const fn new(
        code: &'static str,
        text: &'static str,
        phase: Phase,
        meaning: &'static str,
    ) -> Src {
        Src {
            code,
            text,
            phase,
            meaning,
            cause: None,
            recovery: None,
        }
    }

    /// With its cause.
    pub(crate) const fn cause(self, cause: &'static str) -> Src {
        Src {
            cause: Some(cause),
            ..self
        }
    }

    /// With its recovery.
    pub(crate) const fn recovery(self, recovery: &'static str) -> Src {
        Src {
            recovery: Some(recovery),
            ..self
        }
    }

    /// The code's family.
    #[must_use]
    pub fn family(&self) -> Option<Family> {
        Family::of(self.code)
    }

    /// The serial line: `SRC <code>  <text>`.
    #[must_use]
    pub fn line(&self) -> String {
        format!("SRC {}  {}\n", self.code, self.text)
    }

    /// The code as fixed 8 ASCII bytes for the LIC history record.
    #[must_use]
    pub fn bytes(&self) -> [u8; 8] {
        let mut b = *b"        ";
        for (dst, src) in b.iter_mut().zip(self.code.bytes()) {
            *dst = src;
        }
        b
    }
}
