# Keycord Docs

Keycord is a graphical app for standard [`pass`](https://www.passwordstore.org/) stores. It keeps the same folder layout on disk, works with compatible pass tools, and uses an adaptive GTK interface for keyboard, pointer, and touch use on desktop and mobile Linux.

## Guides

- [Getting Started](getting-started.md): setup, stores, first items, and first searches
- [Search Guide](search.md): plain search, `reg`, and `find`
- [Workflows](workflows.md): editing, OTP, tools, shortcuts, and maintenance
- [Permissions and Backends](permissions-and-backends.md): Integrated vs Host, Flatpak permissions, Git, and key sync
- [Story of Secrets](story-of-secrets.md): code-oriented walkthrough of store creation, entry encryption, unlock paths, and clipboard copy
- [Teams and Organizations](teams-and-organizations.md): shared stores, recipients, onboarding, offboarding, and bootstrap patterns
- [Use Cases](use-cases.md): common setups from personal use to shared stores and admin work

## Standard Layout

Keycord reads and writes normal `pass` stores:

- a store directory such as `~/.password-store`
- one secret per file
- the first line as the password
- later `key: value` lines as structured fields
- `.gpg-id` for store recipients

## Keycord Features

- open one or more password stores and search by name, store, field, regular expression, or structured `find` query
- edit entries with form fields or raw pass-file text, generate passwords, and copy passwords, usernames, or one-time login codes
- add existing stores, create new stores, import passwords on supported systems, or restore a store from Git with the Host backend
- manage store recipients, folder-specific `.gpg-id` files, private keys, FIDO2 security keys, and OpenPGP smartcards
- sync Git-backed stores, manage remotes, sign changes, and inspect history with commit verification details
- use the adaptive GTK interface with keyboard, pointer, or touch on desktop and mobile Linux

## Backend Matrix

| Capability | Integrated | Host | Notes |
| --- | --- | --- | --- |
| Browse and edit standard `pass` stores | Yes | Yes | Both use the standard store layout. |
| Use a custom `pass` command | No | Yes | Linux only; set the command in Preferences. |
| Search, OTP, field-value browser, weak-password tool | Yes | Yes | Search behavior is the same. |
| Manage store recipients and app-managed private keys | Yes | Yes | Host GPG inspection depends on host access. |
| Restore a store from a Git URL in the UI | No | Yes | Linux only; requires host access. |
| `pass import` integration | No | Yes | Linux only; requires the `pass import` extension. |
| Remote Git fetch, merge, and push | Yes | Yes | Linux only; requires host access and a Git-backed store. |
| Smartcard / YubiKey workflows | Yes | Yes | Linux only; Flatpak needs smartcard access. |
| Sync Keycord private keys with host GPG | Yes | Yes | Linux only and host access required. |

## Limits

- Flatpak without host access:
  - Host-only features such as restore-from-Git and `pass import` stay disabled.
  - If Host is selected without host access, Keycord falls back to Integrated behavior.
- Non-Linux builds:
  - Host-only features such as custom `pass`, restore-from-Git, and `pass import` stay hidden.
  - hardware-key workflows stay hidden.
- Flatpak smartcard support:
  - hardware-key actions need PC/SC access
  - password-protected software keys do not
- Layered encryption:
  - this is Keycord-specific
  - other `pass` apps cannot read those items

## Start

1. Read [Getting Started](getting-started.md).
2. Keep [Search Guide](search.md) open while you build queries.
3. Use [Permissions and Backends](permissions-and-backends.md) if a feature is disabled.
