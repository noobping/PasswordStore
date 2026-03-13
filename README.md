
# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)

Keycord works with password stores that use the standard [`pass`](https://www.passwordstore.org/) layout, so existing data can stay in place and remain compatible with established workflows.

- Search entries across more than one store
- Work with structured fields or raw pass-file text
- Generate passwords and copy usernames, secrets, and one-time codes
- Manage recipients (and private keys in the flatpak app)
- Clone and sync Git-backed stores
- Import passwords from other password managers through `pass import` when that extension is available (not in the flatpak app)

## How It Works

Keycord reads and writes standard `pass` stores. It can work in two ways:

- `Integrated` backend: the app reads and writes the store directly. This is the default.
- `Host command` backend: the app runs your chosen `pass` command. In the Flatpak app, this is available when host command execution is permitted.

## Screenshots

![import](screenshots/import.png)

![list](screenshots/list.png)

![Password entry editor](screenshots/file.png)

## Development

Package names differ by distribution. This project was tested with Fedora packages:

```sh
sudo dnf install gpgme-devel clang pkg-config nettle-devel libgpg-error-devel openssl-devel gtk4-devel gcc pkgconf-pkg-config glib2-devel cairo-devel pango-devel libadwaita-devel cargo mold clippy rustfmt \
    git pass pass-otp pinentry pinentry-gnome3 python-pass-import
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
