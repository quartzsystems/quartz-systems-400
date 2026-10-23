// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with System Capacity`: a display of what the machine has. This
//! system has no capacity on demand: processors and memory are what
//! the hardware table reports, and nothing here changes them.

use svctools::DstEnv;
use svctools::dst::common::{Notice, Submenu};
use svctools::panel::Flow;

/// The `Work with System Capacity` menu.
#[must_use]
pub fn work_with_system_capacity() -> Submenu {
    Submenu::new(
        "sst-capacity",
        "Work with System Capacity",
        vec![
            (
                1,
                "Display processor capacity status",
                Some(Box::new(|env: &mut dyn DstEnv| {
                    Flow::Push(Box::new(processor_status(env)))
                })),
            ),
            (2, "Work with temporary capacity", None),
            (3, "Work with permanent capacity", None),
        ],
    )
}

/// The label of the processor count among the environment's
/// `lic_info` lines.
pub const PROCESSORS: &str = "Processors";

/// The processor count the environment reports; 1 when it does not.
fn processors(env: &mut dyn DstEnv) -> String {
    env.lic_info()
        .into_iter()
        .find(|(k, _)| k == PROCESSORS)
        .map_or_else(|| "1".to_string(), |(_, v)| v)
}

fn processor_status(env: &mut dyn DstEnv) -> Notice {
    let processors = processors(env);
    let resources = env.resources().len();
    Notice::new(
        "sst-capacity-processor",
        "Display Processor Capacity Status",
        vec![
            format!("Installed processors . . . . . :  {processors}"),
            format!("Active processors  . . . . . . :  {processors}"),
            format!("Hardware resources reported  . :  {resources}"),
            String::new(),
            "Every installed processor is active: this system has no capacity".into(),
            "held in reserve to be activated later.".into(),
        ],
    )
}
