// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Break messages: the queue's break-handling program runs on the
//! job's stack with the queue, its library and the message key; the
//! system's (`*DSPMSG`) shows the message on the job's display, here
//! the console.

use stg::Client;

use super::Console;

impl Console {
    /// Deliver the breaks waiting for the job.
    pub(crate) fn breaks(&mut self) {
        let waiting = self.link.borrow_mut().with(Client::breaks);
        for (queue, (pgm, lib), m) in waiting {
            if pgm.is_empty() || pgm.starts_with('*') {
                let (date, time) = wm::joblog::stamp_of(m.time);
                self.output(&format!(
                    "\nBreak message on {queue} from {} ({date} {}):\n  {}\n",
                    m.job.1,
                    &time[..8],
                    m.text
                ));
                continue;
            }
            // The program gets the queue name, its library and the key,
            // as IBM's break-handling programs do.
            let (qlib, qname) = queue.split_once('/').unwrap_or(("*LIBL", queue.as_str()));
            let mut name = wm::rec::text_bytes(qname, 10);
            let mut library = wm::rec::text_bytes(qlib, 10);
            let mut key = m.key.to_be_bytes().to_vec();
            let args = [
                name.as_mut_ptr() as u64,
                library.as_mut_ptr() as u64,
                key.as_mut_ptr() as u64,
            ];
            let lib = if lib.is_empty() {
                "*LIBL".to_string()
            } else {
                lib
            };
            let j = self.job();
            if let Err(e) = j.call_program(&pgm, &lib, &args) {
                self.output(&format!(
                    "{}: break-handling program {lib}/{pgm} could not be called.\n",
                    e.msgid()
                ));
            }
            let _ = self.job().clear();
        }
    }
}
