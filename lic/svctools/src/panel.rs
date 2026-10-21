// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A panel is one screen and what Enter and the function keys do on it.
//! A runner keeps a stack of panels: the top one is shown, F12 goes back
//! one, F3 goes back to the first, and a panel can push another, replace
//! itself, or end the whole application with an outcome.

use console::Screen;

use crate::env::DstEnv;
use crate::ui;

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

/// What a panel asks of the runner after a key.
pub enum Flow {
    /// Nothing changes; the message line is cleared.
    Stay,
    /// Stay and show a message.
    Message(String),
    /// Stay and show an error (red).
    Error(String),
    /// Show another panel on top.
    Push(Box<dyn Panel>),
    /// Back to the previous panel.
    Pop,
    /// Back to the first panel.
    PopAll,
    /// Show another panel in this one's place.
    Replace(Box<dyn Panel>),
    /// Forget every panel and start over with this one.
    Reset(Box<dyn Panel>),
    /// Back `n` panels, with a message for the one revealed.
    Unwind(usize, Option<(String, bool)>),
    /// End the application.
    Exit(Outcome),
}

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
    stack: Vec<Box<dyn Panel>>,
    message: Option<(String, bool)>,
}

impl Runner {
    /// Start with one panel.
    #[must_use]
    pub fn new(root: Box<dyn Panel>) -> Self {
        Self {
            stack: vec![root],
            message: None,
        }
    }

    /// The screen to show now.
    pub fn screen(&self, env: &mut dyn DstEnv) -> Screen {
        let top = self.stack.last().expect("never empty");
        let mut s = top.render(env);
        if let Some((text, error)) = &self.message {
            ui::message(&mut s, text, *error);
        }
        s
    }

    /// Name of the panel on top.
    #[must_use]
    pub fn top(&self) -> &'static str {
        self.stack.last().expect("never empty").name()
    }

    /// Panels on the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Enter was pressed with these field contents.
    pub fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Option<Outcome> {
        let flow = self
            .stack
            .last_mut()
            .expect("never empty")
            .enter(fields, env);
        self.apply(flow)
    }

    /// A function key was pressed.
    pub fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Option<Outcome> {
        let flow = self
            .stack
            .last_mut()
            .expect("never empty")
            .function(key, env);
        self.apply(flow)
    }

    fn apply(&mut self, flow: Flow) -> Option<Outcome> {
        self.message = None;
        match flow {
            Flow::Stay => {}
            Flow::Message(m) => self.message = Some((m, false)),
            Flow::Error(m) => self.message = Some((m, true)),
            Flow::Push(p) => self.stack.push(p),
            Flow::Pop => {
                if self.stack.len() == 1 {
                    return Some(Outcome::Exit);
                }
                self.stack.pop();
            }
            Flow::PopAll => {
                if self.stack.len() == 1 {
                    return Some(Outcome::Exit);
                }
                self.stack.truncate(1);
            }
            Flow::Replace(p) => {
                self.stack.pop();
                self.stack.push(p);
            }
            Flow::Reset(p) => {
                self.stack.clear();
                self.stack.push(p);
            }
            Flow::Unwind(n, message) => {
                let keep = self.stack.len().saturating_sub(n).max(1);
                if keep == self.stack.len() && n > 0 {
                    return Some(Outcome::Exit);
                }
                self.stack.truncate(keep);
                self.message = message;
            }
            Flow::Exit(o) => return Some(o),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;

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
