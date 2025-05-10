/* store.rs
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

use crate::dir::discover_store_dir;
use crate::entry::Entry;

use anyhow::{Context, Result, anyhow};
use derivative::Derivative;
use git2::{
    Cred, CredentialType, FetchOptions, MergeOptions, PushOptions, RemoteCallbacks, Repository,
};
use gpgme::{
    Context as GpgContext, DecryptFlags, KeyListMode, PassphraseRequest, PinentryMode, Protocol,
};
use log::{info, warn};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

/// Main handle to the password store.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct PassStore {
    root: Option<PathBuf>,
    #[derivative(Debug = "ignore")]
    repo: Option<RefCell<Repository>>,
    gpg: Option<RefCell<GpgContext>>,
}

impl Clone for PassStore {
    fn clone(&self) -> Self {
        match PassStore::new() {
            Ok(store) => store,
            Err(e) => {
                warn!("Failed to clone PassStore: {}", e);
                PassStore::default()
            }
        }
    }
}

impl Default for PassStore {
    fn default() -> Self {
        PassStore {
            root: None,
            repo: None,
            gpg: None,
        }
    }
}

impl PassStore {
    pub fn new() -> Result<Self> {
        let root = discover_store_dir()?;
        let repo = Repository::discover(&root)?;
        let mut gpg = GpgContext::from_protocol(Protocol::OpenPgp)
            .context("Failed to create GPG context for password store")?;
        gpg.set_armor(true);

        Ok(PassStore {
            root: Some(root),
            repo: Some(repo.into()),
            gpg: Some(gpg.into()),
        })
    }

    fn root(&self) -> Result<&PathBuf, anyhow::Error> {
        let root = self.root.as_ref()
            .ok_or_else(|| anyhow!("Password store root is not initialized"))?;
        Ok(root)
    }

    fn gpg(&self) -> std::cell::RefMut<'_, GpgContext> {
        self.gpg
            .as_ref()
            .expect("GPG context is not initialized")
            .borrow_mut()
    }

    fn repo(&self) -> std::cell::RefMut<'_, Repository> {
        self.repo
            .as_ref()
            .expect("Repository is not initialized")
            .borrow_mut()
    }

    pub fn ok(&self) -> bool {
        self.root.is_some() && self.repo.is_some() && self.gpg.is_some()
    }

    /// Create a new `RemoteCallbacks` instance for SSH authentication.
    fn make_callbacks() -> RemoteCallbacks<'static> {
        let mut cb = RemoteCallbacks::new();

        cb.credentials(|_url, username_from_url, allowed| {
            // Prefer whatever was in the URL (ssh://alice@host/…, git@host:…, etc.)
            let user = username_from_url.unwrap_or("git");

            // If the server is ready for the key, hand it over ⤵
            if allowed.contains(CredentialType::SSH_KEY) {
                return Cred::ssh_key_from_agent(user);
            }
            // Otherwise it’s only asking who we are
            if allowed.contains(CredentialType::USERNAME) {
                return Cred::username(user);
            }

            Err(git2::Error::from_str("No supported authentication method"))
        });

        cb
    }

    /// Return a list of all password entries as relative paths (without the `.gpg` suffix).
    ///
    /// Recursively scans the store for `.gpg` files. Hidden files/dirs and “.”/“..” are excluded.
    /// Returns a sorted vector of entry identifiers (e.g. `"folder/sub/entry"`).
    pub fn list(&self) -> Result<Vec<String>> {
        let root = self.root()?;
        let pattern = root.join("**/*.gpg");
        let opts = glob::MatchOptions {
            require_literal_leading_dot: true,
            ..Default::default()
        };

        let mut entries = Vec::new();
        for item in glob::glob_with(
            pattern
                .to_str()
                .ok_or_else(|| anyhow!("non-UTF-8 store path"))?,
            opts,
        )? {
            let path = match item {
                Ok(p) => p,
                Err(e) => {
                    // Als een pad niet gelezen kan worden (bv. permissieprobleem), skip het.
                    warn!("Skipping unreadable entry during listing: {}", e);
                    continue;
                }
            };
            let rel = path.strip_prefix(&root)?;
            // Sla paden met verborgen componenten of '.'/'..' over
            let has_hidden = rel.components().any(|c| match c {
                std::path::Component::CurDir | std::path::Component::ParentDir => true,
                _ => c.as_os_str().to_string_lossy().starts_with('.'),
            });
            if has_hidden {
                continue;
            }
            // Verwijder de `.gpg` extensie om de entry-ID te verkrijgen
            let id = rel.with_extension("").to_string_lossy().into_owned();
            if !id.is_empty() {
                entries.push(id);
            }
        }
        entries.sort();
        Ok(entries)
    }

    /// Decrypt and retrieve a password entry by its relative path (without the `.gpg` extension).
    ///
    /// Returns an `Entry` containing the password and any extra lines of metadata.
    /// Errors if the entry file is not found or if decryption fails.
    pub fn get(&self, id: &str, passphrase: &str) -> Result<Entry> {
        // 1. Load the .gpg file into `cipher`
        let root = self.root()?;
        let path = root.join(format!("{id}.gpg"));
        let cipher =
            std::fs::read(&path).with_context(|| format!("Failed to read entry `{id}`"))?;

        // 2. Tell GPGME to accept a loop‑back pass‑phrase
        let mut gpg = self.gpg();
        gpg.set_pinentry_mode(PinentryMode::Loopback)?;

        // 3. Temporarily install a provider that writes `passphrase`
        let secret = passphrase.to_owned();
        let mut plain = Vec::new();
        gpg.with_passphrase_provider(
            move |_req: PassphraseRequest, out: &mut dyn Write| {
                writeln!(out, "{secret}")?; // passphrase + '\n'
                Ok(())
            },
            |ctx| {
                // 4. Decrypt while the provider is active
                ctx.decrypt_with_flags(&cipher, &mut plain, DecryptFlags::empty())
            },
        )?; // propagates any GPGME error

        // 5. Convert to UTF‑8 and parse
        let txt = String::from_utf8(plain)?;
        Ok(Entry::from_plaintext(txt))
    }

    /// Check whether a given entry (by relative path without `.gpg`) exists in the store.
    pub fn entry_exists(&self, id: &str) -> bool {
        if self.root.is_none() {
            return false;
        }
        let root = self.root().unwrap();
        let path = root.join(format!("{}.gpg", id));
        println!("Checking existence of entry: '{}'", path.display());
        path.is_file()
    }

    /// Check if the password store exists.
    pub fn exists(&self) -> bool {
        self.root.is_some() && self.root.as_ref().unwrap().is_dir()
    }

    /// Encrypt (for the given recipients) and write an entry. Creates parents as needed.
    pub fn insert(&self, id: &str, entry: &Entry, recipients: &[&str]) -> Result<()> {
        // Resolve keys.
        let mut gpg = self.gpg();
        gpg.set_key_list_mode(KeyListMode::LOCAL | KeyListMode::SIGS)
            .expect("Failed to set key list mode");
        let keys: Vec<_> = recipients
            .iter()
            .map(|r| gpg.get_key(*r))
            .collect::<Result<_, _>>()?;
        if keys.is_empty() {
            return Err(anyhow!("No recipients found for encryption"));
        }

        let mut cipher = Vec::new();
        gpg.encrypt(&keys, &entry.to_plaintext().into_bytes()[..], &mut cipher)?;

        let path = self.root()?.join(format!("{}.gpg", id));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(&path)?.write_all(&cipher)?;

        self.git_add_commit(&format!("Add/Update {}", id))?;
        Ok(())
    }

    /// Remove an entry.
    pub fn remove(&self, id: &str) -> Result<()> {
        let path = self.root()?.join(format!("{}.gpg", id));
        fs::remove_file(&path)?;
        Ok(self.git_add_commit(&format!("Remove {}", id))?)
    }

    /// Rename an existing entry to a new path (move/rename the underlying `.gpg` file).
    ///
    /// Creates any missing directories for the new path. Returns an error if the source entry
    /// does not exist, if the destination already exists, or if the operation fails.
    pub fn rename(&self, old_id: &str, new_id: &str) -> Result<()> {
        // Valideer dat geen van beide paden verboden componenten bevat
        for part in std::path::Path::new(old_id)
            .components()
            .chain(std::path::Path::new(new_id).components())
        {
            match part {
                std::path::Component::CurDir | std::path::Component::ParentDir => {
                    return Err(anyhow!("Invalid path component in rename"));
                }
                _ => {
                    if part.as_os_str().to_string_lossy().starts_with('.') {
                        return Err(anyhow!("Hidden path component not allowed in rename"));
                    }
                }
            }
        }
        let root = self.root()?;
        let src = root.join(format!("{}.gpg", old_id));
        let dst = root.join(format!("{}.gpg", new_id));
        if !src.is_file() {
            return Err(anyhow!("Entry '{}' does not exist", old_id));
        }
        if dst.exists() {
            return Err(anyhow!("Target entry '{}' already exists", new_id));
        }
        // Maak doelmappen aan indien nodig
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, &dst)?;
        // Git-commit voor deze wijziging (stage + commit)
        self.git_add_commit(&format!("Rename {} to {}", old_id, new_id))?;
        Ok(())
    }

    // Git helpers

    /// Fetch all remotes.
    pub fn git_fetch(&self) -> Result<()> {
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(Self::make_callbacks());
        let repo = self.repo();
        for name in repo.remotes()?.iter().flatten() {
            let mut remote = repo.find_remote(name)?;
            remote.fetch(&[] as &[&str], Some(&mut fo), None)?;
        }
        Ok(())
    }

    /// Pull from the branch’s configured upstream.
    /// – No changes…… → Ok
    /// – Fast‑forward… → branch pointer moves
    /// – Diverged………. → real merge commit (errors if conflicts)
    pub fn git_pull(&self) -> Result<()> {
        let repo = self.repo();
        // 1a. Get the current HEAD (e.g. "refs/heads/master")
        let head_ref = repo.head()?;
        let head_name = head_ref
            .shorthand()
            .ok_or_else(|| anyhow!("Detached HEAD"))?; // e.g. "master"

        // 1b. Ask libgit2 for the full remote-tracking ref
        let binding = repo.branch_upstream_name(head_name)?;
        let upstream_refname = binding
            .as_str()
            .ok_or_else(|| anyhow!("No upstream configured"))?; // e.g. "refs/remotes/origin/master"

        // 2. Fetching with Default Refspecs
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(Self::make_callbacks());

        for remote_name in repo.remotes()?.iter().flatten() {
            let mut remote = repo.find_remote(remote_name)?;
            // Passing `&[]` → use the base refspecs (same as `git fetch origin`)
            remote.fetch(&[] as &[&str], Some(&mut fo), None)?;
        }

        // 3a. Find the remote-tracking reference itself
        let fetch_ref = repo.find_reference(upstream_refname)?;

        // 3b. Convert it into an AnnotatedCommit for merge_analysis
        let annotated = repo.reference_to_annotated_commit(&fetch_ref)?; // see discussion: prefer this over peel_to_commit :contentReference[oaicite:7]{index=7}

        // 4a. Analyse up-to-date vs fast-forward vs normal merge
        let (analysis, _) = repo.merge_analysis(&[&annotated])?;

        if analysis.is_up_to_date() {
            // nothing to do
            info!("Already up-to-date");
            return Ok(());
        }

        if analysis.is_fast_forward() {
            // 4b. Fast-forward: move branch pointer + checkout
            let mut head_ref_mut = repo.find_reference(head_ref.name().unwrap())?;
            head_ref_mut.set_target(annotated.id(), "fast-forward")?;
            repo.set_head(head_ref.name().unwrap())?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            return Ok(());
        }

        // 4c. True merge: commit a merge in-repo
        let mut merge_opts = MergeOptions::new();
        repo.merge(&[&annotated], Some(&mut merge_opts), None)?;

        let mut idx = repo.index()?;
        if idx.has_conflicts() {
            return Err(anyhow!("Merge conflicts detected"));
        }
        let tree_oid = idx.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let sig = repo.signature()?;
        let local_commit = {
            let c = repo.reference_to_annotated_commit(&head_ref)?;
            repo.find_commit(c.id())?
        };
        let upstream_commit = repo.find_commit(annotated.id())?;

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Merge {} into {}", upstream_refname, head_name),
            &tree,
            &[&local_commit, &upstream_commit],
        )?;

        // refresh work-tree & clear merge state
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        repo.cleanup_state()?;
        Ok(())
    }

    /// Commit all staged changes with the given message and push.
    pub fn git_add_commit(&self, message: &str) -> Result<()> {
        let repo = self.repo();
        let mut idx = repo.index()?;
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        idx.write()?;
        let oid = idx.write_tree()?;
        let tree = repo.find_tree(oid)?;
        let sig = repo.signature()?;

        let parent = if let Ok(head) = repo.head() {
            Some(head.peel_to_commit()?)
        } else {
            None
        };
        let parents = parent.iter().collect::<Vec<_>>();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
        Ok(())
    }

    /// Push the current branch to its configured upstream.
    pub fn git_push(&self) -> Result<()> {
        let mut cb = RemoteCallbacks::new();
        cb.credentials(|_url, username_from_url, _allowed| {
            Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        });
        let mut po = PushOptions::new();
        po.remote_callbacks(Self::make_callbacks());

        let repo = self.repo();
        let head = repo.head()?;
        let branch = head.shorthand().ok_or_else(|| anyhow!("Detached HEAD"))?;
        let spec = format!("refs/heads/{}", branch);
        let mut remote = repo.find_remote("origin").or_else(|_| {
            repo.remotes()?
                .iter()
                .find(|r| *r == Some("origin"))
                .ok_or_else(|| git2::Error::from_str("Remote 'origin' not found"))?
                .and_then(|_| Some(repo.find_remote("origin")))
                .expect("Remote 'origin' not found")
        })?;
        remote.push(&[&format!("{}:refs/heads/{}", spec, branch)], Some(&mut po))?;
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        self.git_fetch()?;
        self.git_pull()?;
        self.git_push()?;

        Ok(())
    }
}
