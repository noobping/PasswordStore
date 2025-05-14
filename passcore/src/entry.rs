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

impl ToString for Entry {
    fn to_string(&self) -> String {
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

impl Entry {
    pub fn from_secret(secret: impl ExposeSecret<str>) -> Self {
        let mut lines: std::str::Lines<'_> = secret.expose_secret().lines();
        let password = SecretString::from(lines.next().unwrap_or_default().to_string());
        let extra = lines.map(|l| SecretString::from(l.to_string())).collect();
        Self { password, extra }
    }
}
