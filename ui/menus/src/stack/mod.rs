// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The menu stack: what `GO`, F3, F12 and F16 do to the chain of menus
//! a job is showing (`docs/MENUS.md`, "The menu stack"). The display
//! is elsewhere; this is the state.

use crate::Qualified;

/// The system main menu.
pub const MAIN: (&str, &str) = ("QSYS", "MAIN");

/// Where a `GO` goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// `GO lib/name`.
    Menu(Qualified),
    /// `GO *MAIN`: the system main menu.
    Main,
    /// `GO *USER`: the user's initial menu.
    User,
    /// `GO *SIGNOFF`: end the session.
    SignOff,
    /// `GO CMD*`: the menus a generic name matched, already resolved
    /// against the library list by the caller.
    Choice(Vec<Qualified>),
}

/// What a `GO` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Show this menu, now the current one.
    Show(Qualified),
    /// Several menus match: show the list, then `push` the choice.
    Choose(Vec<Qualified>),
    /// End the session.
    SignOff,
    /// No menu to show: the generic name matched nothing, or the user
    /// has no initial menu.
    NotFound,
}

/// The menus a job is showing, the first one at the bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuStack {
    frames: Vec<Qualified>,
    user: Option<Qualified>,
}

impl MenuStack {
    /// A stack showing `first`; `user` is the profile's initial menu
    /// for `GO *USER`, `None` when it is `*SIGNOFF`.
    #[must_use]
    pub fn new(first: Qualified, user: Option<Qualified>) -> Self {
        Self {
            frames: vec![first],
            user,
        }
    }

    /// The menu on the screen.
    #[must_use]
    pub fn current(&self) -> &Qualified {
        self.frames.last().expect("never empty")
    }

    /// The menu shown when the stack started; F3 returns here.
    #[must_use]
    pub fn initial(&self) -> &Qualified {
        &self.frames[0]
    }

    /// How many menus are kept, the initial one included.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// The menus from the initial one to the current one.
    #[must_use]
    pub fn frames(&self) -> &[Qualified] {
        &self.frames
    }

    /// Show `menu`. With `RTNPNT(*YES)` the current menu stays as the
    /// return point for F12; with `*NO` the new menu takes its place.
    pub fn push(&mut self, menu: Qualified, rtnpnt: bool) -> &Qualified {
        if !rtnpnt {
            self.frames.pop();
        }
        self.frames.push(menu);
        self.current()
    }

    /// `GO`.
    pub fn go(&mut self, target: Target, rtnpnt: bool) -> Outcome {
        match target {
            Target::Menu(m) => Outcome::Show(self.push(m, rtnpnt).clone()),
            Target::Main => {
                Outcome::Show(self.push(Qualified::new(MAIN.0, MAIN.1), rtnpnt).clone())
            }
            Target::User => match self.user.clone() {
                Some(u) => Outcome::Show(self.push(u, rtnpnt).clone()),
                None => Outcome::NotFound,
            },
            Target::SignOff => Outcome::SignOff,
            Target::Choice(mut list) => match list.len() {
                0 => Outcome::NotFound,
                1 => Outcome::Show(self.push(list.remove(0), rtnpnt).clone()),
                _ => Outcome::Choose(list),
            },
        }
    }

    /// F3=Exit: back to the initial menu, or `None` when it is already
    /// shown and the stack is finished (the caller returns to whatever
    /// started it, or signs off when that was the sign-on).
    pub fn exit(&mut self) -> Option<&Qualified> {
        if self.frames.len() == 1 {
            return None;
        }
        self.frames.truncate(1);
        Some(self.current())
    }

    /// F12=Cancel: back to the previous menu, or `None` at the initial
    /// one, as `exit`.
    pub fn cancel(&mut self) -> Option<&Qualified> {
        if self.frames.len() == 1 {
            return None;
        }
        self.frames.pop();
        Some(self.current())
    }

    /// F16=System main menu: `GO *MAIN RTNPNT(*YES)`.
    pub fn main(&mut self) -> &Qualified {
        self.push(Qualified::new(MAIN.0, MAIN.1), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: &str) -> Qualified {
        Qualified::qsys(n)
    }

    #[test]
    fn exit_and_cancel_walk_the_chain() {
        let mut s = MenuStack::new(q("MAIN"), Some(q("MAIN")));
        assert_eq!(
            s.go(Target::Menu(q("MAJOR")), true),
            Outcome::Show(q("MAJOR"))
        );
        s.push(q("VERB"), true);
        s.push(q("CMDADD"), true);
        assert_eq!(s.depth(), 4);
        assert_eq!(s.cancel(), Some(&q("VERB")));
        assert_eq!(s.exit(), Some(&q("MAIN")));
        assert_eq!(s.exit(), None);
        assert_eq!(s.cancel(), None);
        assert_eq!(s.current(), &q("MAIN"));
    }

    #[test]
    fn no_return_point_replaces() {
        let mut s = MenuStack::new(q("MAIN"), None);
        s.push(q("MAJOR"), true);
        s.push(q("CMDJOB"), false);
        assert_eq!(s.frames(), &[q("MAIN"), q("CMDJOB")]);
        assert_eq!(s.cancel(), Some(&q("MAIN")));
        // From the initial menu, *NO makes the new menu the initial one.
        s.push(q("MYMENU"), false);
        assert_eq!(s.initial(), &q("MYMENU"));
        assert_eq!(s.exit(), None);
    }

    #[test]
    fn special_targets() {
        let mut s = MenuStack::new(q("CMDJOB"), Some(q("USRMENU")));
        assert_eq!(s.go(Target::Main, true), Outcome::Show(q("MAIN")));
        assert_eq!(s.go(Target::User, true), Outcome::Show(q("USRMENU")));
        assert_eq!(s.go(Target::SignOff, true), Outcome::SignOff);
        assert_eq!(s.main(), &q("MAIN"));
        assert_eq!(s.exit(), Some(&q("CMDJOB")));
        let mut t = MenuStack::new(q("MAIN"), None);
        assert_eq!(t.go(Target::User, true), Outcome::NotFound);
    }

    #[test]
    fn generic_names() {
        let mut s = MenuStack::new(q("MAIN"), None);
        assert_eq!(s.go(Target::Choice(vec![]), true), Outcome::NotFound);
        assert_eq!(
            s.go(Target::Choice(vec![q("CMDJOB")]), true),
            Outcome::Show(q("CMDJOB"))
        );
        let two = vec![q("CMDADD"), q("CMDALC")];
        assert_eq!(
            s.go(Target::Choice(two.clone()), true),
            Outcome::Choose(two)
        );
        assert_eq!(s.current(), &q("CMDJOB"));
    }
}
