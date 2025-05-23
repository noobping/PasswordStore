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
use crate::extension::SecretStringExt;

use anyhow::{Context, anyhow};
use derivative::Derivative;
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{
    BranchType, Cred, CredentialType, FetchOptions, MergeOptions, PushOptions, RemoteCallbacks,
    Repository,
};
use gpgme::{Context as GpgContext, DecryptFlags, KeyListMode, PinentryMode, Protocol};
use log::{info, warn};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretString};
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
    pub fn new() -> anyhow::Result<Self> {
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
        let root = self
            .root
            .as_ref()
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
    fn callbacks() -> RemoteCallbacks<'static> {
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

    /// Read the default recipients from the `.gpg-id` file.
    fn recipients(&self) -> anyhow::Result<Vec<String>> {
        let root = self.root().context("Failed to get password store root")?;
        let path = root.join(".gpg-id");
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read .gpg-id file at {}", path.display()))?;
        // 3. Split op regels, trim en filter lege regels
        let recipients = content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        Ok(recipients)
    }

    /// Return a list of all password entries as relative paths (without the `.gpg` suffix).
    ///
    /// Recursively scans the store for `.gpg` files. Hidden files/dirs and “.”/“..” are excluded.
    /// Returns a sorted vector of entry identifiers (e.g. `"folder/sub/entry"`).
    pub fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }

        let root = self.root()?;
        let pattern = root.join("**/*.gpg");
        let opts = glob::MatchOptions {
            require_literal_leading_dot: true,
            ..Default::default()
        };

        let mut entries = Vec::new();
        for entry in glob::glob_with(
            pattern
                .to_str()
                .ok_or_else(|| anyhow!("non-UTF-8 store path"))?,
            opts,
        )? {
            let path = match entry {
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
    pub fn get(&self, id: &str, passphrase: SecretString) -> anyhow::Result<Entry> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }

        // 1. Read ciphertext
        let cipher = std::fs::read(self.root()?.join(format!("{id}.gpg")))
            .with_context(|| format!("Failed to read entry `{id}`"))?;

        // 2. GPG context
        let mut gpg = self.gpg();
        gpg.set_pinentry_mode(PinentryMode::Loopback)?;

        // 3. Decrypt
        let passphrase_owned = passphrase.to_owned();
        let mut secret = secrecy::SecretBox::<Vec<u8>>::new(Box::new(Vec::new()));

        gpg.with_passphrase_provider(
            move |_req: gpgme::PassphraseRequest<'_>, out: &mut dyn std::io::Write| {
                writeln!(out, "{}", passphrase_owned.expose_secret())?;
                Ok(())
            },
            |ctx| {
                ctx.decrypt_with_flags(&cipher, secret.expose_secret_mut(), DecryptFlags::empty())
            },
        )?;

        // 4. Convert & wipe
        Ok(Entry::from_secret(SecretString::from_secret_utf8(secret)?))
    }

    /// Like `get`, but let GPGME/agent ask you for the passphrase via pinentry.
    pub fn ask(&self, id: &str) -> anyhow::Result<Entry> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }

        // 1. Read the .gpg blob
        let root = self.root()?;
        let path = root.join(format!("{id}.gpg"));
        let cipher =
            std::fs::read(&path).with_context(|| format!("Failed to read entry `{}`", id))?;

        // 2. Force a pinentry dialog instead of loopback
        let mut gpg = self.gpg();
        gpg.set_pinentry_mode(PinentryMode::Ask)?;

        // 3. Decrypt; GPGME will launch your pinentry (GUI/tty) for you
        let mut secret = secrecy::SecretBox::<Vec<u8>>::new(Box::new(Vec::new()));
        gpg.decrypt_with_flags(&cipher, secret.expose_secret_mut(), DecryptFlags::empty())
            .context("Decryption failed")?;

        // 4. Convert & wipe
        Ok(Entry::from_secret(SecretString::from_secret_utf8(secret)?))
    }

    /// Check whether a given entry (by relative path without `.gpg`) exists in the store.
    pub fn exists(&self, id: &str) -> bool {
        if self.root.is_none() {
            return false;
        }
        let root = self.root().unwrap();
        let path = root.join(format!("{}.gpg", id));
        info!("Checking existence of entry: '{}'", path.display());
        path.is_file()
    }

    /// Encrypt (for the given recipients) and write an entry. Creates parents as needed.
    pub fn add(&self, id: &str, entry: &Entry) -> anyhow::Result<()> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }
        let message = if self.exists(id) {
            format!("Update {}", id)
        } else {
            format!("Add {}", id)
        };
        // Resolve keys.
        let mut gpg = self.gpg();
        gpg.set_key_list_mode(KeyListMode::LOCAL | KeyListMode::SIGS)
            .expect("Failed to set key list mode");
        let recipients = self.recipients()?;
        let keys: Vec<_> = recipients
            .iter()
            .map(|r| gpg.get_key(r.clone()))
            .collect::<Result<_, _>>()?;
        if keys.is_empty() {
            return Err(anyhow!("No recipients found for encryption"));
        }
        // Encrypt the entry.
        let mut cipher = Vec::new();
        gpg.encrypt(&keys, &entry.to_string().into_bytes()[..], &mut cipher)?;
        // Write the encrypted entry to the store.
        let path = self.root()?.join(format!("{}.gpg", id));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(&path)?.write_all(&cipher)?;
        // Commit the change to the git repository.
        self.git_add_commit(&message)?;
        Ok(())
    }

    /// Remove an entry.
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }
        let path = self.root()?.join(format!("{}.gpg", id));
        fs::remove_file(&path)?;
        Ok(self.git_add_commit(&format!("Remove {}", id))?)
    }

    /// Rename an existing entry to a new path (move/rename the underlying `.gpg` file).
    ///
    /// Creates any missing directories for the new path. Returns an error if the source entry
    /// does not exist, if the destination already exists, or if the operation fails.
    pub fn rename(&self, old_id: &str, new_id: &str) -> anyhow::Result<()> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }

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
    fn git_fetch(&self) -> anyhow::Result<()> {
        println!("Fetching all remotes...");

        let mut fo = FetchOptions::new();
        fo.remote_callbacks(Self::callbacks());
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
    fn git_pull(&self) -> anyhow::Result<()> {
        println!("Pulling from upstream...");

        let repo = self.repo();
        // 1a. Get the current HEAD (e.g. "refs/heads/master")
        let head_ref = repo.head()?;
        let head_name = head_ref
            .shorthand()
            .ok_or_else(|| anyhow!("Detached HEAD"))?; // e.g. "master"
        println!("HEAD: {}", head_name);

        // 1b. Ask libgit2 for the full remote-tracking ref
        let binding = repo.branch_upstream_name(head_name)?;
        let upstream_refname = binding
            .as_str()
            .ok_or_else(|| anyhow!("No upstream configured"))?; // e.g. "refs/remotes/origin/master"
        println!("Upstream: {}", upstream_refname);

        // 2. Fetching with Default Refspecs
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(Self::callbacks());

        for remote_name in repo.remotes()?.iter().flatten() {
            println!("Fetching remote {}...", remote_name);
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
            println!("Fast-forwarding...");
            // 4b. Fast-forward: move branch pointer + checkout
            let mut head_ref_mut = repo.find_reference(head_ref.name().unwrap())?;
            head_ref_mut.set_target(annotated.id(), "fast-forward")?;
            repo.set_head(head_ref.name().unwrap())?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            println!("Fast-forward completed successfully");
            return Ok(());
        }

        println!("Merging...");
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

        println!("Merge completed successfully");
        Ok(())
    }

    /// Commit all staged changes with the given message and push.
    fn git_add_commit(&self, message: &str) -> anyhow::Result<()> {
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
    fn git_push(&self) -> anyhow::Result<()> {
        println!("Pushing to upstream...");

        let mut cb = RemoteCallbacks::new();
        cb.credentials(|_url, username_from_url, _allowed| {
            Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        });
        let mut po = PushOptions::new();
        po.remote_callbacks(Self::callbacks());

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

    pub fn sync(&self) -> anyhow::Result<()> {
        if !self.ok() {
            return Err(anyhow!("PassStore is not initialized"));
        }

        self.git_fetch()?;
        self.git_pull()?;
        self.git_push()?;

        Ok(())
    }

    pub fn from_git(repo_url: String) -> anyhow::Result<PassStore> {
        // 1. Bepaal waar de store moet staan (bijv. ~/.password-store)
        let root = discover_store_dir()?;

        // 2. Zorg dat de directory leeg is (om een echte clone te doen)
        if root.exists() {
            // als er al iets in staat, liever niet klungelen
            if root.read_dir()?.next().is_some() {
                return Err(anyhow!("Password store directory {:?} is not empty", root));
            }
        } else {
            // maak ‘m aan als 'ie nog niet bestaat
            fs::create_dir_all(&root).context("Failed to create password store directory")?;
        }

        // 3. Clone met onze SSH-callbacks
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(PassStore::callbacks());

        let repo = RepoBuilder::new()
            .fetch_options(fo)
            .clone(&repo_url, &root)
            .context("Failed to clone repository")?;
        PassStore::ensure_local_branch(&repo)?;
        PassStore::ensure_tracking(&repo)?;

        // 4. Zet GPG klaar
        let mut gpg = GpgContext::from_protocol(Protocol::OpenPgp)
            .context("Failed to create GPG context for password store")?;
        gpg.set_armor(true);

        // 5. Return een geconfigureerde PassStore
        Ok(PassStore {
            root: Some(root),
            repo: Some(repo.into()),
            gpg: Some(gpg.into()),
        })
    }

    fn ensure_local_branch(repo: &Repository) -> Result<(), git2::Error> {
        // 1. Wat zit er nu aan HEAD?
        if let Ok(head) = repo.head() {
            if head.is_branch() {
                // Bestaat die refs/heads/<name> écht?
                if let Some(name) = head.shorthand() {
                    if repo.find_branch(name, BranchType::Local).is_ok() {
                        println!("Local branch '{}' already exists.", name);
                        return Ok(()); // ✔️ alles in orde
                    }
                }
            }
        }

        // 2. HEAD is detached of de branchfile ontbreekt → fallback op origin/HEAD
        let origin_head = repo.find_reference("refs/remotes/origin/HEAD")?;
        let sym = origin_head
            .symbolic_target()
            .ok_or_else(|| git2::Error::from_str("origin/HEAD is not symbolic"))?;
        let default_branch = sym.trim_start_matches("refs/remotes/origin/"); // b.v. "main"

        // 2a. Maak lokale branch als die er nog niet is
        if repo.find_branch(default_branch, BranchType::Local).is_err() {
            println!("Creating local branch '{}' from 'origin/{}'", default_branch, default_branch);
            let commit = origin_head.peel_to_commit()?;
            repo.branch(default_branch, &commit, false)?;
        }

        // 2b. Checkout en HEAD eraan hangen
        let branch_ref = format!("refs/heads/{}", default_branch);
        repo.set_head(&branch_ref)?;
        repo.checkout_head(Some(CheckoutBuilder::default().force()))?;
        Ok(())
    }

    /// Zorg dat de huidige lokale branch een geldige upstream heeft.
    /// – Bestaat er al één? -> niets doen.
    /// – Anders: koppel hem aan `origin/<branch>`.
    /// – Mocht zelfs die niet bestaan, val terug op waar `origin/HEAD` naar wijst.
    fn ensure_tracking(repo: &Repository) -> anyhow::Result<String> {
        let head = repo.head()?; // current HEAD
        let name = head
            .shorthand()
            .ok_or_else(|| anyhow::anyhow!("detached HEAD"))?;

        // 1. Upstream al geconfigureerd? -> klaar.
        if repo.branch_upstream_name(name).is_ok() {
            println!("Upstream for branch '{}' is already configured.", &name);
            return Ok(name.to_owned());
        }

        // 2. Probeer origin/<name>
        let remote_full = format!("refs/remotes/origin/{}", name);
        if repo.find_reference(&remote_full).is_ok() {
            println!(
                "No upstream for branch '{}', setting to '{}'",
                name, remote_full
            );
            let mut local = repo.find_branch(name, BranchType::Local)?;
            local.set_upstream(Some(&format!("origin/{}", name)))?; // schrijft .git/config
            return Ok(remote_full);
        }

        // 3. Laatste redmiddel: pak waar origin/HEAD naartoe wijst (meestal 'main')
        let origin_head = repo.find_reference("refs/remotes/origin/HEAD")?;
        if let Some(sym) = origin_head.symbolic_target() {
            println!(
                "No upstream for branch '{}', falling back to '{}'",
                name, sym
            );
            let default_branch = sym.trim_start_matches("refs/remotes/origin/");
            let commit = origin_head.peel_to_commit()?;
            // maak lokale branch als die nog niet bestaat
            if repo.find_branch(default_branch, BranchType::Local).is_err() {
                repo.branch(default_branch, &commit, false)?;
            }
            // checkout & HEAD eraan hangen
            repo.set_head(&format!("refs/heads/{}", default_branch))?;
            repo.checkout_head(Some(CheckoutBuilder::default().force()))?;

            return Ok(default_branch.to_owned());
        }
        Ok(name.to_owned())
    }
}
