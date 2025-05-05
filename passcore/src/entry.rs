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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub password: String,
    /// Remaining lines (metadata) are kept verbatim to preserve compatibility.
    pub extra: Vec<String>,
}

impl Entry {
    pub fn from_plaintext<S: AsRef<str>>(data: S) -> Self {
        let mut lines = data.as_ref().lines();
        let password = lines.next().unwrap_or_default().to_string();
        let extra = lines.map(|l| l.to_string()).collect();
        Self { password, extra }
    }

    pub fn to_plaintext(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.password);
        out.push('\n');
        for l in &self.extra {
            out.push_str(l);
            out.push('\n');
        }
        out
    }
}
