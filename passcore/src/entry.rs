/* entry.rs
 *
 * Copyright 2025 noobping
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0
 */

use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct Entry {
    pub password: SecretString,
    /// Remaining lines (metadata) are kept verbatim to preserve compatibility.
    pub extra: Vec<SecretString>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            password: SecretString::new("".into()),
            extra: vec![],
        }
    }
}

impl Entry {
    pub fn new<S: AsRef<str>>(password: S, extra: Vec<S>) -> Self {
        let password = SecretString::from(password.as_ref().to_string());
        let extra = extra
            .into_iter()
            .map(|l| SecretString::from(l.as_ref().to_string()))
            .collect();
        Self { password, extra }
    }

    pub fn from_lines<S: AsRef<str> + std::default::Default>(lines: Vec<S>) -> Self {
        let mut lines = lines.into_iter();
        let password = SecretString::from(lines.next().unwrap_or_default().as_ref().to_string());
        let extra = lines.map(|l| SecretString::from(l.as_ref().to_string())).collect();
        Self { password, extra }
    }

    // /// Build an `Entry` from a `Vec` of already‑split lines.
    // /// * The **first** line is the password.
    // /// * All remaining lines are copied verbatim into `extra`
    // ///   so the original file format stays intact.
    // pub fn from_lines<S: AsRef<str>>(lines: Vec<S>) -> Self {
    //     // consume the Vec so we don’t leave another allocation behind
    //     let mut iter = lines.into_iter();

    //     // first line → password (empty if no lines at all)
    //     let password = match iter.next() {
    //         Some(p) => SecretString::new(p.as_ref().to_owned().into()),
    //         None => SecretString::new(String::new().into()),
    //     };

    //     // everything else → extra
    //     let extra = iter
    //         .map(|l| SecretString::new(l.as_ref().to_owned().into()))
    //         .collect();

    //     Self { password, extra }
    // }

    pub fn from_plaintext<S: AsRef<str>>(plaintext: S) -> Self {
        let mut lines = plaintext.as_ref().lines();
        let password = SecretString::from(lines.next().unwrap_or_default().to_string());
        let extra = lines.map(|l| SecretString::from(l.to_string())).collect();
        Self { password, extra }
    }

    pub fn from_secret(secret: impl ExposeSecret<str>) -> Self {
        let mut lines: std::str::Lines<'_> = secret.expose_secret().lines();
        let password = SecretString::from(lines.next().unwrap_or_default().to_string());
        let extra = lines.map(|l| SecretString::from(l.to_string())).collect();
        Self { password, extra }
    }

    pub fn to_plaintext(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.password.expose_secret());
        out.push('\n');
        for l in &self.extra {
            out.push_str(l.expose_secret());
            out.push('\n');
        }
        out
    }
}
