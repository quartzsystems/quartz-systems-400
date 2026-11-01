// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the `ADMIN` job's live environment (`work/httpadm`) is made
//! of, without the link: the IFS as `httpd`'s files, dates and times
//! as a job shows them, command outcomes as messages and the
//! instances' jobs from the active job table. Tested on any host.

pub mod files;
pub mod reply;
pub mod stamp;
