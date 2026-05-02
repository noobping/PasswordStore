# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Flathub version](https://img.shields.io/flathub/v/io.github.noobping.keycord)](https://flathub.org/apps/details/io.github.noobping.keycord)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)
[![Windows](https://github.com/noobping/keycord/actions/workflows/win.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/win.yml)

Browse and edit password stores.

Keycord works with password folders that use the standard [`pass`](https://www.passwordstore.org/) layout. Existing stores stay on disk as normal pass stores, so you can keep using compatible tools.

- Open one or more password stores and search by name, store, field, regular expression, or structured `find` query
- Edit entries with form fields or raw pass-file text, generate passwords, and copy passwords, usernames, or one-time login codes
- Add existing stores, create new stores, import passwords on supported systems, or restore a store from Git with the Host backend
- Manage store recipients, folder-specific `.gpg-id` files, private keys, FIDO2 security keys, and OpenPGP smartcards
- Sync Git-backed stores, manage remotes, sign changes, and inspect history with commit verification details
- Use the adaptive GTK interface with keyboard, pointer, or touch on desktop and mobile Linux

![list](screenshots/list.png)

## Documentation

Start with the [Getting Started guide](docs/getting-started.md), then explore the following sections:

- [Search](docs/search.md): how to find outdated or insecure accounts
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
