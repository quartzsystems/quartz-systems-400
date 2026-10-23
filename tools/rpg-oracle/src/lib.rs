// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG differential oracle's shared half: what a corpus member
//! asks for (`case`), how a compile listing is read and masked
//! (`listing`), and the normalized outcome both systems are reduced to
//! (`outcome`). The `rpg-oracle` binary fills outcomes from an IBM i;
//! the conformance kit fills them from QS/400 and compares.

#![deny(missing_docs)]
#![allow(clippy::many_single_char_names)]

pub mod case;
pub mod listing;
pub mod outcome;

pub use case::Case;
pub use outcome::Outcome;
