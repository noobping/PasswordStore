# Permissions And Backends

This page lists features that depend on the Integrated backend, the Host backend, Flatpak host access, smartcard access, or Linux-only host-command features.

## Backend Overview

### Integrated backend

The Integrated backend reads and writes the store directly. It is the default.

Use it when you want:

- direct store access without relying on a host `pass` command,
- app-managed private keys,
- structured editing, search, OTP, and tools,
- Keycord-managed store-recipient updates.

### Host backend

The Host backend runs your configured `pass` command. It is available on Linux only.

Use it when you need:

- a custom `pass` command,
- restore-from-Git in the UI,
- `pass import`,
- compatibility with existing host-side `pass` extensions or wrappers.

## Capability Matrix

| Capability | Integrated | Host | Extra requirement |
| --- | --- | --- | --- |
| Browse, search, copy, edit, OTP, tools | Yes | Yes | None |
| Use a custom `pass` command | No | Yes | Linux only; configure the command in Preferences |
| Create or attach local stores in the UI | Yes | Yes | At least one recipient is required for a new store |
| Restore a store from Git in the UI | No | Yes | Linux only; host access plus `git` |
| `pass import` integration | No | Yes | Linux only; `pass import` must be available in the configured command |
| Manage Git remotes | Yes | Yes | Linux only in the UI; host access required for remote network operations |
| Remote Git sync | Yes | Yes | Linux only; host access, clean repo, checked-out branch, and remotes |
| Smartcard / YubiKey workflows | Yes | Yes | Smartcard access in Flatpak |
| Sync Keycord keys with host GPG | Yes | Yes | Linux only and host access required |

## Flatpak Permissions

### Host access

Without host access, Linux host-driven features are limited.

What host access unlocks:

- Host backend behavior,
- host programs such as `gpg`,
- `pass import`,
- restore-from-Git,
- remote Git fetch, merge, and push.

Keycord exposes this command when host access is missing:

```sh
flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord
```

### Smartcard access

Hardware-key actions need PC/SC access in Flatpak builds.

Keycord exposes this command when smartcard access is missing:

```sh
flatpak override --user --socket=pcsc io.github.noobping.keycord
```

The app asks for a restart after enabling smartcard access.

## Host Backend Notes

### Custom host command

The Host backend command is configurable on Linux. Keycord splits it like a shell command line, and the configured program must still behave like `pass` because Keycord appends normal operations such as show, insert, move, remove, init, and import.

Examples:

```text
pass
/usr/bin/pass
/path/to/custom-pass-wrapper
```

### `pass import`

On Linux, the import page is populated from:

```sh
pass import --list
```

through your configured host command.

If no importers are detected, the import UI stays unavailable.

### Restore from Git

The **Restore password store** action is a Linux-only Host feature because it runs `git clone` into the folder you choose.

## Git Behavior

### Local repository handling

On Linux, when Keycord creates a new store by saving recipients into a folder that does not yet have a `.gpg-id` or `.git`, it initializes a Git repository for that store.

### Remote Git status and sync

On Linux, Keycord can inspect Git-backed stores and manage remotes. Remote sync requires:

- a Git repository,
- at least one remote,
- a checked-out branch,
- no uncommitted local changes.

When sync runs, Keycord:

1. fetches each remote with `--prune`,
2. merges the current branch from each remote,
3. pushes `HEAD` back to each remote.

If the repo is dirty, detached, or missing an initial commit, Keycord stops and tells you what to fix.

### Git signing and private-key unlock

On Linux, Integrated workflows may need a managed private key unlocked before Keycord can sign a Git commit associated with an entry or recipient change.

If the unlock prompt is dismissed, the save can continue without a Git signature.

## Store Recipients And Layered Encryption

### Normal recipient handling

Stores use `.gpg-id` for recipients. Keycord accepts recipient values such as:

- fingerprints,
- key handles,
- user IDs like `Alice Example <alice@example.com>`.

### Require all selected keys

Keycord can mark a store so every selected managed key must be unlocked. This uses layered encryption and adds Keycord-specific metadata to `.gpg-id`.

Use this only when you explicitly want Keycord-only behavior.

Important warning:

- other `pass` apps will not be able to read those items.

## App-Managed Private Keys

Keycord can manage:

- password-protected private keys stored by the app,
- Linux hardware-backed OpenPGP keys,
- public-key imports bound to connected hardware keys on Linux.

The store-key UI supports:

- generate private key,
- add hardware key on Linux,
- import hardware public key on Linux,
- import from clipboard,
- import from file.

## Sync Private Keys With Host GPG

This feature is Linux-only.

When enabled, Keycord first aligns its private-key list with the host GPG private keys, then continues keeping them in step.

Important constraints:

- host access must be available,
- every synced host key must be password-protected before Keycord can store it,
- the first host-to-app sync can remove app-only password-protected keys that are missing from the host keyring,
- later app-to-host sync can import or delete keys so the host matches the app-managed set.

Use this if you want one managed set of software keys across Keycord and the host keyring.

## Hardware Keys

Keycord supports connected OpenPGP smartcards and YubiKeys on Linux.

Use cases:

- add a connected hardware key directly,
- import a matching hardware public key file,
- bind a public key export to the currently connected token.

Flatpak builds need smartcard access for these workflows.

## Troubleshooting Checklist

If a feature is disabled:

1. Check whether it is Host-only.
2. On Flatpak, confirm host access or smartcard access.
3. Check whether the current store has Git metadata and remotes.
4. Check whether the repo is dirty or detached.
5. Confirm that required private keys are present and unlocked.

## Next Reading

- [Getting Started](getting-started.md)
- [Workflows](workflows.md)
- [Use Cases](use-cases.md)
