
# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)

Keycord works with password stores that use the standard [`pass`](https://www.passwordstore.org/) layout, so existing data can stay in place and remain compatible with established workflows.

- Search entries across more than one store
- Work with structured fields or raw pass-file text
- Generate passwords and copy usernames, secrets, and one-time codes
- Manage recipients (and private keys in the flatpak app)
- Clone and sync Git-backed stores (not in the flatpak app)
- Import passwords from other password managers through `pass import` when that extension is available (not in the flatpak app)

## How It Works

Keycord reads and writes standard `pass` stores. On a normal build, it can work in two ways:

- `Integrated` backend: the app reads and writes the store directly. This is the default.
- `Host command` backend: the app runs your chosen `pass` command (wich can come from a Docker container if you want) instead.

## Build Modes

There are a few important runtime differences depending on how you build the app.

| Build mode | Best for | Notes |
| --- | --- | --- |
| Standard build | Most users and developers | Default build. Uses the integrated backend by default, can switch to a custom `pass` command in Preferences, supports Git clone/sync, and can use `pass import` when available. |
| `flatpak` feature | Sandboxed / Containerized environments | Uses the integrated backend only and includes built-in private-key management, avoiding external dependencies. |
| `setup` feature | Self-built installs | Adds an in-app action to add or remove the built binary from the app menu. |

## Screenshots

### Password entry editor

![Password entry editor](screenshots/file.png)

### Preferences in a Flatpak build

![Preferences in a Flatpak build](screenshots/preferences1.png)

### Preferences in a standard build

![Preferences in a standard build](screenshots/preferences2.png)

## Development

Package names differ by distribution. This project was tested with Fedora packages:

```sh
sudo dnf install gpgme-devel clang pkg-config nettle-devel libgpg-error-devel openssl-devel \
    gtk4-devel gcc pkgconf-pkg-config \
    glib2-devel cairo-devel pango-devel libadwaita-devel \
    cargo mold git pass pinentry pinentry-gnome3 python-pass-import \
    clippy rustfmt
```

### Running And Building

Run the default build:

```sh
cargo run
```

Build with the optional local installer action:

```sh
cargo build --release --features setup
```

### Project Layout

| Path | Purpose |
| --- | --- |
| `src/backend` | Reading, writing, deleting, renaming, and recipient management for password entries |
| `src/password` | Entry model, list loading, editor flow, OTP support, password generation, undo |
| `src/store` | Store creation, store list management, recipient editing, import flows |
| `src/window` | Main window, navigation, actions, preferences, Git UI, logs |
| `src/preferences` | Stored app settings such as backend choice, store paths, templates, and generator settings |
| `src/support` | Background-task helpers, UI helpers, Git helpers, and `pass import` support |
| `src/private_key` | Flatpak-only dialogs and flows for private-key unlocking |
| `data` | GTK UI definition, app metadata, icons, and resources |
