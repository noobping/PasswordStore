/* pass.rs
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

use crate::store::PassStore;
use anyhow::Result;

pub struct Pass {
    store: Option<PassStore>,
}

impl Pass {
    pub fn new() -> Result<Self> {
        let store = PassStore::new()?;
        Ok(Pass { store: Some(store) })
    }

    pub fn store(&mut self) -> &mut PassStore {
        if self.store.is_none() {
            self.store = Some(PassStore::default());
        }
        self.store.as_mut().unwrap()
    }
}

impl Default for Pass {
    fn default() -> Self {
        Pass { store: None }
    }
}
