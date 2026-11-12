// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A panel is one screen and what Enter and the function keys do on it.
//! A stack of panels shows the top one: F12 goes back one, F3 goes back
//! to the first, and a panel can push another, replace itself, or end
//! the whole application with an outcome of the caller's.
//!
//! [`Stack`] is the stack and its message line over any panel type.
//! [`Panel`] and [`Runner`] are the ready form for a caller whose
//! panels ask one environment type `E` for what they show; a caller
//! whose environment is a borrowed trait object keeps a panel trait of
//! its own over the same [`Stack`] and [`Flow`].

use console::Screen;

use crate::ui;

/// What a panel asks of the stack after a key. `P` is a panel, `O` the
/// application's outcome.
pub enum Flow<P, O> {
    /// Nothing changes; the message line is cleared.
    Stay,
    /// Stay and show a message.
    Message(String),
    /// Stay and show an error (red).
    Error(String),
    /// Show another panel on top.
    Push(P),
    /// Back to the previous panel.
    Pop,
    /// Back to the first panel.
    PopAll,
    /// Show another panel in this one's place.
    Replace(P),
    /// Forget every panel and start over with this one.
    Reset(P),
    /// Back `n` panels, with a message for the one revealed.
    Unwind(usize, Option<(String, bool)>),
    /// End the application.
    Exit(O),
}

/// How an application ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End<O> {
    /// Back from the first panel (F3 or F12 on it): to whoever started
    /// the application.
    Left,
    /// A panel ended it.
    Outcome(O),
}

/// The stack of panels and the message line.
pub struct Stack<P> {
    stack: Vec<P>,
    message: Option<(String, bool)>,
}

impl<P> Stack<P> {
    /// Start with one panel.
    #[must_use]
    pub fn new(root: P) -> Self {
        Self {
            stack: vec![root],
            message: None,
        }
    }

    /// The panel on top.
    #[must_use]
    pub fn top(&self) -> &P {
        self.stack.last().expect("never empty")
    }

    /// The panel on top, to give it a key.
    pub fn top_mut(&mut self) -> &mut P {
        self.stack.last_mut().expect("never empty")
    }

    /// Panels on the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The message line to show: its text and whether it is an error.
    #[must_use]
    pub fn message(&self) -> Option<(&str, bool)> {
        self.message.as_ref().map(|(m, e)| (m.as_str(), *e))
    }

    /// Put the message line, if there is one, on the top panel's screen.
    pub fn put_message(&self, screen: &mut Screen) {
        if let Some((text, error)) = self.message() {
            ui::message(screen, text, error);
        }
    }

    /// Do what the top panel asked. `Some` when the application ended.
    pub fn apply<O>(&mut self, flow: Flow<P, O>) -> Option<End<O>> {
        self.message = None;
        match flow {
            Flow::Stay => {}
            Flow::Message(m) => self.message = Some((m, false)),
            Flow::Error(m) => self.message = Some((m, true)),
            Flow::Push(p) => self.stack.push(p),
            Flow::Pop => {
                if self.stack.len() == 1 {
                    return Some(End::Left);
                }
                self.stack.pop();
            }
            Flow::PopAll => {
                if self.stack.len() == 1 {
                    return Some(End::Left);
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
                    return Some(End::Left);
                }
                self.stack.truncate(keep);
                self.message = message;
            }
            Flow::Exit(o) => return Some(End::Outcome(o)),
        }
        None
    }
}

/// One screen's behavior over an environment `E`, ending with `O`.
pub trait Panel<E: ?Sized, O> {
    /// Name, for logs and golden files.
    fn name(&self) -> &'static str;
    /// The screen, without the message line.
    fn render(&self, env: &mut E) -> Screen;
    /// Enter with the input fields in order.
    fn enter(&mut self, fields: &[String], env: &mut E) -> PanelFlow<E, O>;
    /// A function key. F3 exits, F12 cancels, unless overridden.
    fn function(&mut self, key: u8, env: &mut E) -> PanelFlow<E, O> {
        let _ = env;
        match key {
            3 => Flow::PopAll,
            12 => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

/// What a [`Panel`] answers a key with.
pub type PanelFlow<E, O> = Flow<Box<dyn Panel<E, O>>, O>;

/// A stack of [`Panel`]s.
pub struct Runner<E: ?Sized, O> {
    stack: Stack<Box<dyn Panel<E, O>>>,
}

impl<E: ?Sized, O> Runner<E, O> {
    /// Start with one panel.
    #[must_use]
    pub fn new(root: Box<dyn Panel<E, O>>) -> Self {
        Self {
            stack: Stack::new(root),
        }
    }

    /// The screen to show now.
    pub fn screen(&self, env: &mut E) -> Screen {
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
    pub fn enter(&mut self, fields: &[String], env: &mut E) -> Option<End<O>> {
        let flow = self.stack.top_mut().enter(fields, env);
        self.stack.apply(flow)
    }

    /// A function key was pressed.
    pub fn function(&mut self, key: u8, env: &mut E) -> Option<End<O>> {
        let flow = self.stack.top_mut().function(key, env);
        self.stack.apply(flow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment: a system name.
    struct Env(&'static str);

    #[derive(Debug, PartialEq, Eq)]
    enum Done {
        Finished,
    }

    struct Counter(u32);

    impl Panel<Env, Done> for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }
        fn render(&self, env: &mut Env) -> Screen {
            ui::text("Counter", env.0, &[format!("{}", self.0)], None, "")
        }
        fn enter(&mut self, fields: &[String], _env: &mut Env) -> PanelFlow<Env, Done> {
            match fields.first().map(String::as_str) {
                Some("push") => Flow::Push(Box::new(Counter(self.0 + 1))),
                Some("done") => Flow::Exit(Done::Finished),
                Some("bad") => Flow::Error("no".into()),
                Some("back2") => Flow::Unwind(2, Some(("went back".into(), false))),
                _ => Flow::Stay,
            }
        }
    }

    fn line(r: &Runner<Env, Done>, env: &mut Env, n: usize) -> String {
        r.screen(env).dump().lines().nth(n).unwrap().to_string()
    }

    #[test]
    fn stack_and_keys() {
        let mut env = Env("SYSTEM1");
        let mut r = Runner::new(Box::new(Counter(0)));
        assert_eq!(r.enter(&["push".into()], &mut env), None);
        assert_eq!(r.enter(&["push".into()], &mut env), None);
        assert_eq!(r.depth(), 3);
        assert_eq!(r.top(), "counter");
        assert!(line(&r, &mut env, 3).starts_with(" 2"));
        assert!(line(&r, &mut env, 1).ends_with("System:  SYSTEM1 "));
        assert_eq!(r.function(12, &mut env), None);
        assert_eq!(r.depth(), 2);
        assert_eq!(r.enter(&["bad".into()], &mut env), None);
        assert!(line(&r, &mut env, 23).starts_with("no"));
        assert_eq!(r.function(3, &mut env), None);
        assert_eq!(r.depth(), 1);
        assert!(line(&r, &mut env, 23).trim().is_empty());
        assert_eq!(r.function(3, &mut env), Some(End::Left));
        assert_eq!(r.function(12, &mut env), Some(End::Left));
        assert_eq!(
            r.enter(&["done".into()], &mut env),
            Some(End::Outcome(Done::Finished))
        );
    }

    #[test]
    fn unwind_reveals_a_panel_with_a_message() {
        let mut env = Env("SYSTEM1");
        let mut r = Runner::new(Box::new(Counter(0)));
        for _ in 0..3 {
            r.enter(&["push".into()], &mut env);
        }
        assert_eq!(r.enter(&["back2".into()], &mut env), None);
        assert_eq!(r.depth(), 2);
        assert!(line(&r, &mut env, 23).starts_with("went back"));
        // More than there are: the first panel stays.
        assert_eq!(r.enter(&["back2".into()], &mut env), None);
        assert_eq!(r.depth(), 1);
        assert_eq!(r.enter(&["back2".into()], &mut env), Some(End::Left));
    }
}
