// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing the operating system: the device type and device, the
//! confirmation, the language group, `Install the Operating System`
//! (take the defaults or change the install options), and the status
//! display with its stages while the `QS00SS1` load is restored. The
//! IPL options follow once the system values exist.

use console::{Attr, Screen};

use crate::dst::common::{Confirm, Select, Submenu};
use crate::env::DstEnv;
use crate::install::{OS_STEPS, Progress, os_status};
use crate::panel::{Flow, Outcome, Panel, Runner};
use crate::ui::{self, FormLine};

/// The language groups the install offers. 2924 is English; the table
/// is here so others can be added.
pub const LANGUAGES: [(&str, &str); 1] = [("2924", "English")];

/// The kind of install device (`Install Device Type Selection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    /// Tape drives (`TAPnn`).
    Tape,
    /// Optical drives and USB media (`OPTnn`).
    Optical,
    /// Virtual devices: an image catalog's volume or a raw image
    /// (`OPTVRTnn`, `TAPVRTnn`).
    Virtual,
}

impl DeviceKind {
    /// For messages.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Tape => "tape",
            Self::Optical => "optical",
            Self::Virtual => "virtual",
        }
    }
}

/// A device with install media in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallDevice {
    /// Resource name, `OPT01`, `TAPVRT01`.
    pub name: String,
    /// Its kind.
    pub kind: DeviceKind,
    /// Where it is: the device node or image.
    pub location: String,
    /// What the media holds, one line (`Install media V1R0M0 (QS00SS1 ...)`).
    pub media: String,
    /// What installing from it does to this system: a fresh install, a
    /// slip install or an upgrade, one line.
    pub action: String,
    /// Why it cannot be installed from (a downgrade), if so.
    pub refused: Option<String>,
}

/// The choices of `Install the Operating System` and `Specify Install
/// Options`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsOptions {
    /// The language feature.
    pub language: String,
    /// The device name.
    pub device: String,
    /// Install every licensed program on the media afterwards.
    pub licpgm_all: bool,
    /// Clear the job and output queues of a replaced system.
    pub clear_queues: bool,
}

impl OsOptions {
    /// The defaults for `device` and `language`.
    #[must_use]
    pub fn defaults(device: &str, language: &str) -> Self {
        Self {
            language: language.to_string(),
            device: device.to_string(),
            licpgm_all: false,
            clear_queues: false,
        }
    }
}

/// The operating system installer, starting at the device type.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(device_type_menu()))
}

fn pick(env: &mut dyn DstEnv, kind: DeviceKind) -> Flow {
    let devices = env.install_devices(kind);
    match devices.len() {
        0 => Flow::Error(format!(
            "No {} device holds QS/400 install media",
            kind.text()
        )),
        1 => Flow::Push(Box::new(confirm_os(devices[0].clone()))),
        _ => Flow::Push(Box::new(select_device(devices))),
    }
}

/// `Install Device Type Selection`.
#[must_use]
pub fn device_type_menu() -> Submenu {
    Submenu::new(
        "install-device-type",
        "Install Device Type Selection",
        vec![
            (1, "Tape", Some(Box::new(|env| pick(env, DeviceKind::Tape)))),
            (
                2,
                "Optical",
                Some(Box::new(|env| pick(env, DeviceKind::Optical))),
            ),
            (
                3,
                "Virtual device",
                Some(Box::new(|env| pick(env, DeviceKind::Virtual))),
            ),
        ],
    )
}

/// `Select Install Device`: when more than one device holds media.
#[must_use]
pub fn select_device(devices: Vec<InstallDevice>) -> Select {
    let rows = devices
        .iter()
        .map(|d| format!("{:<10} {}", d.name, clip(&d.media, 52)))
        .collect();
    Select::new(
        "install-device",
        "Select Install Device",
        "Type 1 beside the device to install from, then press Enter.",
        "Opt  Device     Media",
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            Flow::Push(Box::new(confirm_os(devices[chosen[0]].clone())))
        }),
    )
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `Confirm Install of the Operating System`.
#[must_use]
pub fn confirm_os(device: InstallDevice) -> Confirm {
    let mut lines = vec![
        format!("Install device . . . :  {}  {}", device.name, clip(&device.location, 40)),
        format!("Media  . . . . . . . :  {}", clip(&device.media, 50)),
        format!("Install action . . . :  {}", clip(&device.action, 50)),
        String::new(),
    ];
    match &device.refused {
        Some(why) => {
            lines.push("The operating system cannot be installed from this media:".into());
            lines.push(clip(why, 76));
        }
        None if device.action.starts_with("Fresh") => {
            lines.push("ASP 1 is initialized for the operating system: any objects".into());
            lines.push("left on it are lost.".into());
        }
        None => {
            lines.push("The operating system's objects are replaced one by one; user".into());
            lines.push("libraries and their objects are kept.".into());
        }
    }
    let refused = device.refused.clone();
    Confirm::new(
        "install-os-confirm",
        "Confirm Install of the Operating System",
        lines,
        Box::new(move |_env: &mut dyn DstEnv| match &refused {
            Some(why) => Err(why.clone()),
            None => Ok(String::new()),
        }),
    )
    .then(move |_env| Flow::Push(Box::new(select_language(device.clone()))))
}

/// `Select a Language Group`.
#[must_use]
pub fn select_language(device: InstallDevice) -> Select {
    let rows: Vec<String> = LANGUAGES
        .iter()
        .map(|(code, name)| format!("{code}   {name}"))
        .collect();
    Select::new(
        "install-language",
        "Select a Language Group",
        "Type 1 beside the language feature to install, then press Enter.",
        "Opt  Feature  Language",
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let (code, name) = LANGUAGES[chosen[0]];
            Flow::Push(Box::new(confirm_language(device.clone(), code, name)))
        }),
    )
}

/// `Confirm Language Feature Selection`: F10 goes on to `Install the
/// Operating System`.
#[must_use]
pub fn confirm_language(device: InstallDevice, code: &'static str, name: &'static str) -> Confirm {
    Confirm::new(
        "install-language-confirm",
        "Confirm Language Feature Selection",
        vec![
            format!("Language feature . . . :  {code}  {name}"),
            String::new(),
            "Press F10 to install the operating system with this language.".into(),
        ],
        Box::new(|_env: &mut dyn DstEnv| Ok(String::new())),
    )
    .then(move |_env| {
        Flow::Push(Box::new(InstallOsPanel {
            options: OsOptions::defaults(&device.name, code),
            device: device.clone(),
        }))
    })
}

/// `Install the Operating System`: take the defaults or change the
/// install options; the date and time.
pub struct InstallOsPanel {
    /// The device.
    pub device: InstallDevice,
    /// The choices so far.
    pub options: OsOptions,
}

impl Panel for InstallOsPanel {
    fn name(&self) -> &'static str {
        "install-os"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let (date, time) = env.clock();
        let mut s = ui::form(
            "Install the Operating System",
            &env.system_name(),
            &[
                FormLine::new("Install option", 1, "1"),
                FormLine::new("Date (YYYY-MM-DD)", 10, date),
                FormLine::new("Time (HH:MM:SS)", 8, time),
            ],
            "F3=Exit   F12=Cancel",
        );
        let col = ui::BODY_COL;
        s.put(ui::BODY_ROW + 4, col + 4, "1=Take the defaults (no other prompts)", Attr::GREEN);
        s.put(ui::BODY_ROW + 5, col + 4, "2=Change the install options", Attr::GREEN);
        s.put(
            ui::BODY_ROW + 7,
            col,
            &format!("Install device  . . . :  {}", self.device.name),
            Attr::GREEN,
        );
        s.put(
            ui::BODY_ROW + 8,
            col,
            &format!("Language feature  . . :  {}", self.options.language),
            Attr::GREEN,
        );
        s.put(
            ui::BODY_ROW + 10,
            col,
            "Type choices, press Enter.",
            Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let get = |i: usize| fields.get(i).map_or("", String::as_str).trim().to_string();
        let option = get(0);
        if let Err(e) = env.set_clock(&get(1), &get(2)) {
            return Flow::Error(e);
        }
        match option.as_str() {
            "1" => finish(env, &self.device, &self.options),
            "2" => Flow::Push(Box::new(SpecifyOptionsPanel {
                device: self.device.clone(),
                options: self.options.clone(),
            })),
            other => Flow::Error(format!("Install option '{other}' is not 1 or 2")),
        }
    }
}

/// `Specify Install Options`.
pub struct SpecifyOptionsPanel {
    /// The device.
    pub device: InstallDevice,
    /// The choices so far.
    pub options: OsOptions,
}

impl Panel for SpecifyOptionsPanel {
    fn name(&self) -> &'static str {
        "install-options"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let o = &self.options;
        let mut s = ui::form(
            "Specify Install Options",
            &env.system_name(),
            &[
                FormLine::new("Licensed programs", 1, if o.licpgm_all { "2" } else { "1" }),
                FormLine::new("Job and output queues", 1, if o.clear_queues { "2" } else { "1" }),
            ],
            "F3=Exit   F12=Cancel",
        );
        let col = ui::BODY_COL;
        s.put(ui::BODY_ROW + 3, col + 4, "Licensed programs:", Attr::GREEN);
        s.put(ui::BODY_ROW + 4, col + 6, "1=Install the operating system only", Attr::GREEN);
        s.put(ui::BODY_ROW + 5, col + 6, "2=Then install every licensed program on the media", Attr::GREEN);
        s.put(ui::BODY_ROW + 6, col + 4, "Job and output queues of a replaced system:", Attr::GREEN);
        s.put(ui::BODY_ROW + 7, col + 6, "1=Keep   2=Clear", Attr::GREEN);
        s.put(ui::BODY_ROW + 9, col, "Type choices, press Enter.", Attr::GREEN);
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let pick = |i: usize| -> Result<bool, String> {
            match fields.get(i).map_or("", String::as_str).trim() {
                "1" => Ok(false),
                "2" => Ok(true),
                v => Err(format!("'{v}' is not 1 or 2")),
            }
        };
        match (pick(0), pick(1)) {
            (Ok(lp), Ok(clear)) => {
                self.options.licpgm_all = lp;
                self.options.clear_queues = clear;
                finish(env, &self.device, &self.options)
            }
            (Err(e), _) | (_, Err(e)) => Flow::Error(e),
        }
    }
}

fn finish(env: &mut dyn DstEnv, device: &InstallDevice, options: &OsOptions) -> Flow {
    match run_install_from(env, device, options) {
        Ok(()) => Flow::Exit(Outcome::Continue),
        Err(e) => Flow::Error(e),
    }
}

/// Install from `device` with the status display.
pub fn run_install_from(
    env: &mut dyn DstEnv,
    device: &InstallDevice,
    options: &OsOptions,
) -> Result<(), String> {
    let mut progress = Progress::new(&OS_STEPS);
    progress.note = format!(
        "Install device {}, language feature {}",
        device.name, options.language
    );
    env.show(os_status(env, &progress));
    env.install_os_from(device, options, &mut |step, env| {
        progress.done(step);
        env.show(os_status(env, &progress));
        env.pace();
    })
}

/// The operating system with the defaults from the first device that
/// holds media (an automatic install).
pub fn run_install(env: &mut dyn DstEnv, language: &str) -> Result<(), String> {
    let device = [DeviceKind::Optical, DeviceKind::Virtual, DeviceKind::Tape]
        .into_iter()
        .find_map(|k| env.install_devices(k).into_iter().find(|d| d.refused.is_none()));
    if let Some(d) = device {
        let o = OsOptions::defaults(&d.name, language);
        return run_install_from(env, &d, &o);
    }
    let mut progress = Progress::new(&OS_STEPS);
    progress.note = format!("Language feature {language}");
    env.install_os(&mut |step, env| {
        progress.done(step);
        env.show(os_status(env, &progress));
        env.pace();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn dev(name: &str, kind: DeviceKind) -> InstallDevice {
        InstallDevice {
            name: name.into(),
            kind,
            location: "/dev/sr0".into(),
            media: "Install media V1R0M0 (QS00SS1)".into(),
            action: "Fresh install of V1R0M0".into(),
            refused: None,
        }
    }

    #[test]
    fn the_sequence_installs_and_continues() {
        let mut e = MemEnv::new("QS400");
        e.devices = vec![dev("OPT01", DeviceKind::Optical)];
        let mut r = start();
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert!(r.screen(&mut e).dump().contains("No tape device holds"));
        assert_eq!(r.enter(&f(&["2"]), &mut e), None);
        assert_eq!(r.top(), "install-os-confirm");
        assert!(r.screen(&mut e).dump().contains("OPT01"));
        assert_eq!(r.function(10, &mut e), None);
        assert_eq!(r.top(), "install-language");
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert_eq!(r.top(), "install-language-confirm");
        assert_eq!(r.function(10, &mut e), None);
        assert_eq!(r.top(), "install-os");
        assert_eq!(
            r.enter(&f(&["3", "2026-10-31", "12:00:00"]), &mut e),
            None
        );
        assert!(r.screen(&mut e).dump().contains("is not 1 or 2"));
        assert_eq!(
            r.enter(&f(&["1", "2026-10-31", "12:00:00"]), &mut e),
            Some(Outcome::Continue)
        );
        assert!(e.actions.contains(&"install-os OPT01 2924 lp=false clear=false".to_string()));
        assert!(e.shown.last().unwrap().contains("X  Stage 6"));
    }

    #[test]
    fn options_and_devices() {
        let mut e = MemEnv::new("QS400");
        let mut refused = dev("TAPVRT01", DeviceKind::Virtual);
        refused.refused = Some("the media holds V1R0M0, older than V1R1M0".into());
        e.devices = vec![dev("OPTVRT01", DeviceKind::Virtual), refused];
        let mut r = start();
        r.enter(&f(&["3"]), &mut e);
        assert_eq!(r.top(), "install-device");
        r.enter(&f(&["", "1"]), &mut e);
        assert!(r.screen(&mut e).dump().contains("cannot be installed"));
        r.function(10, &mut e);
        assert!(r.screen(&mut e).dump().contains("older than"));
        r.function(12, &mut e);
        r.enter(&f(&["1", ""]), &mut e);
        r.function(10, &mut e);
        r.enter(&f(&["1"]), &mut e);
        r.function(10, &mut e);
        r.enter(&f(&["2", "2026-10-31", "12:00:00"]), &mut e);
        assert_eq!(r.top(), "install-options");
        assert_eq!(r.enter(&f(&["2", "2"]), &mut e), Some(Outcome::Continue));
        assert!(e.actions.contains(&"install-os OPTVRT01 2924 lp=true clear=true".to_string()));
        // Without devices the automatic install falls back on the environment.
        let mut e = MemEnv::new("QS400");
        run_install(&mut e, "2924").unwrap();
        assert_eq!(e.actions, vec!["install-os"]);
    }
}
