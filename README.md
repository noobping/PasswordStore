# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Flathub version](https://img.shields.io/flathub/v/io.github.noobping.keycord)](https://flathub.org/apps/details/io.github.noobping.keycord)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)
[![Windows](https://github.com/noobping/keycord/actions/workflows/win.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/win.yml)

Browse and edit password stores.

Keycord works with password stores that use the standard [`pass`](https://www.passwordstore.org/) layout, so you can keep using the password folders you already have.

- Open, search, and edit one or more password stores
- Make passwords and copy passwords, usernames, and one-time login codes
- Edit entries with easy fields or as plain text
- Check for weak passwords and see repeated details like usernames, emails, and web addresses
- Add an existing store, make a new one, or restore one from Git
- Import passwords from other apps on supported Linux setups
- Choose which keys unlock a store, including password-protected keys, FIDO security keys, and OpenPGP smartcards
- Create and import keys
- Sync Git-backed stores, manage Git remotes, and sign Git commits
- For extra-sensitive stores, require more than one key to open them

![list](screenshots/list.png)

## Documentation

Start with the [Getting Started guide](docs/getting-started.md), then explore the following sections:

- [Search](docs/search.md): how to find outdated or insecure accounds  
- [Workflows](docs/workflows.md): how to do things in Keycord
- [Permissions & Backends](docs/permissions-and-backends.md): application environment
- [Use Cases](docs/use-cases.md): practical examples and short tutorials
- [Teams & Organizations](docs/teams-and-organizations.md): manage shared stores and collaboration

## Development

Package names differ by distribution. This project was tested with Fedora packages:

```sh
sudo dnf install gpgme-devel clang pkg-config pkgconf-pkg-config nettle-devel libgpg-error-devel openssl-devel gtk4-devel gdk-pixbuf2-devel gcc gcc-c++ make gettext glib2-devel cairo-devel capnproto capnproto-devel pcsc-lite-devel pango-devel libadwaita-devel cargo mold clippy rustfmt \
    cmake libcbor-devel hidapi-devel libfido2-devel pcsc-lite pcsc-lite-ccid systemd-devel git pass pass-otp pinentry pinentry-gnome3 python-pass-import
```
