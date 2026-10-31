// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Linux-facing pieces: console devices, mounts, signals, power.

pub mod blk;
pub mod console;
pub mod efivar;
pub mod ifs;
pub mod mounts;
pub mod power;
pub mod rtc;
pub mod serial;
pub mod sys;
