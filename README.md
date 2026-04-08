# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Flathub version](https://img.shields.io/flathub/v/io.github.noobping.keycord)](https://flathub.org/apps/details/io.github.noobping.keycord)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)
[![Windows](https://github.com/noobping/keycord/actions/workflows/win.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/win.yml)

Browse and edit password stores.

Keycord works with password folders that use the standard [`pass`](https://www.passwordstore.org/) layout, so you can keep using the folders you already have.

- Open one or more password folders, then search, filter, and edit what is inside
- Create passwords and quickly copy passwords, usernames, and one-time login codes
- Edit entries with simple fields or as plain text
- Find weak passwords and spot repeated usernames, email addresses, and website links
- Add a password folder you already have, create a new one, or restore one from Git
- Import passwords from other apps on supported Linux systems
- Choose which keys can unlock a folder, including password-protected keys, security keys, and OpenPGP smartcards
- Create new keys or import keys you already have
- Use Git to sync password folders, choose where they sync, and sign your changes
- Inspect change history to see what changed and whether a commit was verified
- For extra-sensitive folders, require more than one key before they open

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
