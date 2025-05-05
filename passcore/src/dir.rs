/* dir.rs
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

use anyhow::{Result, anyhow};
use directories::BaseDirs;
use std::env;
use std::path::PathBuf;

/// Default sub‑directory used by *pass* when PASSWORD_STORE_DIR is not set.
const DEFAULT_STORE_DIR: &str = ".password-store";

/// Determine the password‑store directory.
pub fn discover_store_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("PASSWORD_STORE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = BaseDirs::new().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    Ok(base.home_dir().join(DEFAULT_STORE_DIR))
}
