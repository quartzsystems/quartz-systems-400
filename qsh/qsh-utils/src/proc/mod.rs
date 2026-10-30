// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shell and process utilities: `basename date dirname echo env
//! printenv expr id logname locale nohup ps pwdx sleep test true false
//! uname xargs`.

pub mod basename;
pub mod date;
pub mod echo;
pub mod env;
pub mod expr;
pub mod id;
pub mod locale;
pub mod nohup;
pub mod ps;
pub mod pwdx;
pub mod sleep;
pub mod test;
pub mod truefalse;
pub mod uname;
pub mod xargs;
