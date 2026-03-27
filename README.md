# Keycord

![License](https://img.shields.io/badge/license-GPLv3+-blue.svg)
[![Flathub version](https://img.shields.io/flathub/v/io.github.noobping.keycord)](https://flathub.org/apps/details/io.github.noobping.keycord)
[![Linux](https://github.com/noobping/keycord/actions/workflows/linux.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/linux.yml)
[![Windows](https://github.com/noobping/keycord/actions/workflows/win.yml/badge.svg)](https://github.com/noobping/keycord/actions/workflows/win.yml)

Browse and edit password stores.

Keycord works with password stores that use the standard [`pass`](https://www.passwordstore.org/) layout, so existing data can stay in place and remain compatible with established workflows.

- Generate passwords and copy usernames, secrets, and one-time codes
- Search across multiple password stores
- Identify outdated accounts and weak passwords
- Work with both structured fields and raw pass file text
- Optionally sync with Git
- Optionally sync with the system keyring
- Optionally use layered encryption
- Use password-protected or Linux hardware-backed OpenPGP private keys
- Manage recipients and private keys
- Sign commits and manage Git remotes

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
    pcsc-lite pcsc-lite-ccid git pass pass-otp pinentry pinentry-gnome3 python-pass-import
```
