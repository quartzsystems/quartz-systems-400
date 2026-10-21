// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing SCS.

use super::{AHPP, AVPP, CR, CSP, FF, NL, PAGE, PP, SCD, SCG, SHF, SLD, SVF, TRN};

/// SCS bytes, built a command at a time.
#[derive(Clone, Debug, Default)]
pub struct Encoder {
    /// What was written.
    pub out: Vec<u8>,
}

impl Encoder {
    /// The format commands a file starts with: CP037 graphics, the page
    /// width and length, the line density and the pitch.
    pub fn header(&mut self, width: u16, len: u16, lpi: u8, cpi: u8) {
        // GCGID 697, CPGID 37.
        self.out.extend_from_slice(&[CSP, SCG, 6, 1, 0x02, 0xB9, 0x00, 0x25]);
        self.out
            .extend_from_slice(&[CSP, SHF, 2, u8::try_from(width.min(255)).unwrap_or(255)]);
        self.out
            .extend_from_slice(&[CSP, SVF, 2, u8::try_from(len.min(255)).unwrap_or(255)]);
        self.out.extend_from_slice(&[CSP, SLD, 2, 72 / lpi.max(1)]);
        self.out.extend_from_slice(&[CSP, PAGE, 4, SCD, 0, cpi]);
    }

    /// New line.
    pub fn nl(&mut self) {
        self.out.push(NL);
    }

    /// Carriage return.
    pub fn cr(&mut self) {
        self.out.push(CR);
    }

    /// Form feed.
    pub fn ff(&mut self) {
        self.out.push(FF);
    }

    /// To line `n` (absolute vertical presentation position).
    pub fn line(&mut self, n: u16) {
        self.out
            .extend_from_slice(&[PP, AVPP, u8::try_from(n.min(255)).unwrap_or(255)]);
    }

    /// To column `n` (absolute horizontal presentation position).
    pub fn column(&mut self, n: u16) {
        self.out
            .extend_from_slice(&[PP, AHPP, u8::try_from(n.min(255)).unwrap_or(255)]);
    }

    /// CP037 graphic characters. Control bytes are sent transparently
    /// so they print as nothing rather than act.
    pub fn text(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            let n = rest.iter().take_while(|b| **b >= 0x40).count();
            self.out.extend_from_slice(&rest[..n]);
            rest = &rest[n..];
            let c = rest.iter().take_while(|b| **b < 0x40).count().min(255);
            if c > 0 {
                self.out.push(TRN);
                self.out.push(u8::try_from(c).unwrap_or(0));
                self.out.extend(rest[..c].iter().map(|_| 0x40));
                rest = &rest[c..];
            }
        }
    }
}
