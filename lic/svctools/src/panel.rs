// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A panel is one screen and what Enter and the function keys do on it.
//! A runner keeps a stack of panels: the top one is shown, F12 goes back
//! one, F3 goes back to the first, and a panel can push another, replace
//! itself, or end the whole application with an outcome. The stack and
//! what a panel may ask of it are the screen kit's (`scrkit::panel`);
//! the panels here ask a [`DstEnv`] for what they show.

use console::Screen;
use scrkit::panel::{End, Stack};

use crate::env::DstEnv;

/// How an application ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Option 1: IPL from the load source.
    PerformIpl,
    /// Option 2: run the operating system installer.
    InstallOs,
    /// Option 8: the installer without prompts.
    AutoInstall,
    /// F3 on the first panel: back to whoever started the application.
    Exit,
    /// The installer wants DST.
    Dst,
    /// The LIC is installed: re-IPL from the load source.
    Reboot,
    /// The step is done: the IPL goes on.
    Continue,
}

/// What a panel asks of the runner after a key: stay (with a message
/// or an error), push, pop, replace, reset, unwind, or exit with an
/// [`Outcome`].
pub type Flow = scrkit::panel::Flow<Box<dyn Panel>, Outcome>;

/// One screen's behavior.
pub trait Panel {
    /// Name, for logs and golden files.
    fn name(&self) -> &'static str;
    /// The screen, without the message line.
    fn render(&self, env: &mut dyn DstEnv) -> Screen;
    /// Enter with the input fields in order.
    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow;
    /// A function key. F3 exits, F12 cancels, unless overridden.
    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        let _ = env;
        match key {
            3 => Flow::PopAll,
            12 => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

/// The stack of panels and the message line.
pub struct Runner {
    stack: Stack<Box<dyn Panel>>,
}

/// Back from the first panel is [`Outcome::Exit`].
fn outcome(end: Option<End<Outcome>>) -> Option<Outcome> {
    end.map(|e| match e {
        End::Left => Outcome::Exit,
        End::Outcome(o) => o,
    })
}

impl Runner {
    /// Start with one panel.
    #[must_use]
    pub fn new(root: Box<dyn Panel>) -> Self {
        Self {
            stack: Stack::new(root),
        }
    }

    /// The screen to show now.
    pub fn screen(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = self.stack.top().render(env);
        self.stack.put_message(&mut s);
        s
    }

    /// Name of the panel on top.
    #[must_use]
    pub fn top(&self) -> &'static str {
        self.stack.top().name()
    }

    /// Panels on the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.depth()
    }

    /// Enter was pressed with these field contents.
    pub fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Option<Outcome> {
        let flow = self.stack.top_mut().enter(fields, env);
        outcome(self.stack.apply(flow))
    }

    /// A function key was pressed.
    pub fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Option<Outcome> {
        let flow = self.stack.top_mut().function(key, env);
        outcome(self.stack.apply(flow))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;
    use crate::ui;

    struct Counter(u32);

    impl Panel for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }
        fn render(&self, env: &mut dyn DstEnv) -> Screen {
            ui::text(
                "Counter",
                &env.system_name(),
                &[format!("{}", self.0)],
                None,
                "",
            )
        }
        fn enter(&mut self, fields: &[String], _env: &mut dyn DstEnv) -> Flow {
            match fields.first().map(String::as_str) {
                Some("push") => Flow::Push(Box::new(Counter(self.0 + 1))),
                Some("done") => Flow::Exit(Outcome::PerformIpl),
                Some("bad") => Flow::Error("no".into()),
                _ => Flow::Stay,
            }
        }
    }

    #[test]
    fn stack_and_keys() {
        let mut env = MemEnv::default();
        let mut r = Runner::new(Box::new(Counter(0)));
        assert_eq!(r.enter(&["push".into()], &mut env), None);
        assert_eq!(r.enter(&["push".into()], &mut env), None);
        assert_eq!(r.depth(), 3);
        assert!(
            r.screen(&mut env)
                .dump()
                .lines()
                .nth(3)
                .unwrap()
                .starts_with(" 2")
        );
        assert_eq!(r.function(12, &mut env), None);
        assert_eq!(r.depth(), 2);
        assert_eq!(r.enter(&["bad".into()], &mut env), None);
        assert!(
            r.screen(&mut env)
                .dump()
                .lines()
                .nth(23)
                .unwrap()
                .starts_with("no")
        );
        assert_eq!(r.function(3, &mut env), None);
        assert_eq!(r.depth(), 1);
        assert!(
            r.screen(&mut env)
                .dump()
                .lines()
                .nth(23)
                .unwrap()
                .trim()
                .is_empty()
        );
        assert_eq!(r.function(3, &mut env), Some(Outcome::Exit));
        assert_eq!(
            r.enter(&["done".into()], &mut env),
            Some(Outcome::PerformIpl)
        );
    }
}
