// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job process's Linux plumbing: the native stack's guard and the
//! system clock the SNTP client adjusts.

pub mod clock;
pub mod stack;
