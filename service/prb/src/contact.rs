// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Support contact information (`WRKCNTINF`, `CHGCNTINF`) and the
//! service providers (`WRKSRVPVD`), kept as `key=value` lines.

use std::fmt::Write as _;

/// One service provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Control point name.
    pub cpname: String,
    /// Network identifier.
    pub netid: String,
    /// How it is reached: `*IPCNN`, `*DIRECT` or `*NONE`.
    pub connection: String,
    /// Description.
    pub text: String,
}

/// Who the service provider calls back, and the providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Company.
    pub company: String,
    /// Contact person.
    pub contact: String,
    /// Telephone numbers: primary, alternative.
    pub phone: [String; 2],
    /// Fax numbers: primary, alternative.
    pub fax: [String; 2],
    /// Street address.
    pub street: String,
    /// City or locality, state or province.
    pub city: String,
    /// Country or region.
    pub country: String,
    /// Postal code.
    pub postal: String,
    /// Language version of the reports (`2924`).
    pub language: String,
    /// Media for fixes: `*AUTOMATIC`, `*OPTICAL`, `*TAPE`.
    pub media: String,
    /// E-mail address.
    pub email: String,
    /// The service providers.
    pub providers: Vec<Provider>,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            company: String::new(),
            contact: String::new(),
            phone: [String::new(), String::new()],
            fax: [String::new(), String::new()],
            street: String::new(),
            city: String::new(),
            country: String::new(),
            postal: String::new(),
            language: "2924".into(),
            media: "*AUTOMATIC".into(),
            email: String::new(),
            providers: vec![Provider {
                cpname: "QSERVICE".into(),
                netid: "*NETATR".into(),
                connection: "*NONE".into(),
                text: "Quartz Systems service".into(),
            }],
        }
    }
}

fn clean(s: &str) -> String {
    s.replace(['\n', '\t'], " ")
}

impl Contact {
    /// The stored text.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::from("QS400CNT 1\n");
        let fields: [(&str, &str); 13] = [
            ("company", &self.company),
            ("contact", &self.contact),
            ("phone1", &self.phone[0]),
            ("phone2", &self.phone[1]),
            ("fax1", &self.fax[0]),
            ("fax2", &self.fax[1]),
            ("street", &self.street),
            ("city", &self.city),
            ("country", &self.country),
            ("postal", &self.postal),
            ("language", &self.language),
            ("media", &self.media),
            ("email", &self.email),
        ];
        for (k, v) in fields {
            let _ = writeln!(out, "{k}={}", clean(v));
        }
        for p in &self.providers {
            let _ = writeln!(
                out,
                "provider={}\t{}\t{}\t{}",
                clean(&p.cpname),
                clean(&p.netid),
                clean(&p.connection),
                clean(&p.text)
            );
        }
        out
    }

    /// From the stored text; empty text is the default.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        if !text.starts_with("QS400CNT") {
            return Self::default();
        }
        let mut c = Self {
            providers: Vec::new(),
            ..Self::default()
        };
        for line in text.lines().skip(1) {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.to_string();
            match k {
                "company" => c.company = v,
                "contact" => c.contact = v,
                "phone1" => c.phone[0] = v,
                "phone2" => c.phone[1] = v,
                "fax1" => c.fax[0] = v,
                "fax2" => c.fax[1] = v,
                "street" => c.street = v,
                "city" => c.city = v,
                "country" => c.country = v,
                "postal" => c.postal = v,
                "language" => c.language = v,
                "media" => c.media = v,
                "email" => c.email = v,
                "provider" => {
                    let f: Vec<&str> = v.split('\t').collect();
                    if f.len() == 4 {
                        c.providers.push(Provider {
                            cpname: f[0].into(),
                            netid: f[1].into(),
                            connection: f[2].into(),
                            text: f[3].into(),
                        });
                    }
                }
                _ => {}
            }
        }
        c
    }

    /// The display's label and value lines.
    #[must_use]
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Company", self.company.clone()),
            ("Contact", self.contact.clone()),
            ("Primary telephone", self.phone[0].clone()),
            ("Alternative telephone", self.phone[1].clone()),
            ("Primary fax", self.fax[0].clone()),
            ("Alternative fax", self.fax[1].clone()),
            ("Street address", self.street.clone()),
            ("City/State", self.city.clone()),
            ("Country or region", self.country.clone()),
            ("Postal code", self.postal.clone()),
            ("Language version", self.language.clone()),
            ("Media for fixes", self.media.clone()),
            ("E-mail address", self.email.clone()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut c = Contact {
            company: "Quartz Systems".into(),
            contact: "Cody".into(),
            ..Contact::default()
        };
        c.phone[0] = "555 0100".into();
        assert_eq!(Contact::decode(&c.encode()), c);
        assert_eq!(Contact::decode(""), Contact::default());
    }
}
