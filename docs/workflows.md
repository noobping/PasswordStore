# Workflows

This page covers common tasks after you have at least one store configured.

## Create, Open, And Organize Items

### Create a new item

Press `Ctrl+N` and enter a store path such as:

```text
personal/github
work/vpn/admin
```

If more than one store is configured, Keycord lets you choose the target store first.

### Rename, move, and delete

From the list view:

- `F2` renames the selected entry.
- `Ctrl+M` moves the selected entry.
- `Delete` deletes the selected entry.

### Copy from the list

From the list view, `Ctrl+C` copies the selected entry's password line.

## Edit Structured Fields Or Raw Text

### Structured editor

Use the standard editor for:

- password,
- username,
- OTP,
- dynamic `key: value` fields,
- password generation,
- quick copy actions.

Known username aliases such as `user:` and `login:` are normalized into the username field.

### Raw editor

Press `Ctrl+Shift+R` to open the raw pass file.

Use it when you need to:

- preserve exact layout,
- edit non-structured notes,
- inspect the literal `otpauth://` line,
- fix unusual data imported from elsewhere.

### Save behavior

`Ctrl+S` is context-sensitive:

- on a password page it saves the current pass file,
- on the store-recipient page it saves store keys,
- on the home page it syncs stores when Git sync is available.

## Templates, Cleanup, And Username Fallback

### New password template

Preferences includes **New Password Template**. The template becomes the body after the password line when you create a new entry.

Typical template:

```text
username:
email:
url:
```

Keycord can also apply missing template fields to an existing structured pass file without overwriting fields that are already present.

### Clean pass file

Press `Ctrl+Shift+K` to clean the current pass file.

The cleanup action removes blank:

- username lines,
- structured field lines,
- OTP lines.

If you enable **Clear empty fields before save** in Preferences, Keycord performs that cleanup automatically before validation and save.

### Username fallback

When an entry has no encrypted username field, Keycord can show one of two fallbacks:

- **Use folder name**: the last folder segment becomes the displayed username.
- **Use file name**: the pass file name becomes the displayed username.

This affects display and label-derived behavior when the pass file itself has no username field.

## Generate Passwords

Press `Ctrl+Shift+G` in the password editor to generate a password.

The generator uses saved preferences for:

- total length,
- minimum lowercase letters,
- minimum uppercase letters,
- minimum numbers,
- minimum symbols.

Setting a minimum to `0` disables that character class. If every minimum is `0`, Keycord still keeps the generator usable by re-enabling lowercase internally.

## Work With OTP / TOTP

Press `Ctrl+Shift+O` to add an OTP field to the current entry.

How it works:

- Keycord stores OTP data as an `otpauth://` URL.
- In the structured editor, Keycord shows a live code and countdown.
- Clicking the OTP row switches it into edit mode so you can update the secret.
- Blank OTP secrets are rejected on save.

Use `find otp` in search when you need every entry that has OTP enabled.

## Search, Visibility, Reload, And Sync

### Search

Press `Ctrl+F` to show or hide the search bar.

Keycord supports:

- plain label search,
- `reg` regex search,
- `find` structured search.

You can also launch Keycord with a search query directly:

```sh
keycord 'find url contains github'
```

See [Search Guide](search.md) for the full syntax.

### Hidden and duplicate entries

Press `Ctrl+H` to toggle both hidden and duplicate entries on the home list.

Use this when:

- you keep dot-prefixed or otherwise hidden entries in the store,
- you intentionally keep duplicate labels across multiple stores,
- you want an audit-oriented view instead of the cleaner default list.

### Refresh and sync

- `F5` reloads the current list context.
- `Ctrl+Shift+S` syncs Git-backed stores from the home page when Git sync is available.

Git sync only succeeds when each syncable store:

- has a Git repository,
- has at least one remote,
- has a checked-out branch,
- has no uncommitted local changes.

If the repo is dirty or needs branch repair, use Git on the host first, then return to Keycord.

## Tools Page

Press `Ctrl+T` to open Tools.

The Tools page is split into **Tools** and **Logs** groups.

### Browse field values

This tool reads the currently loaded list and shows:

- searchable field names,
- unique values for each field,
- how many entries share a value.

Selecting a value applies an exact `find` query back to the home list.

This tool excludes raw OTP URLs from the field catalog.

### Find weak passwords

This tool scans the first password line of the currently loaded list and flags entries that fail Keycord's basic checks.

It reports these cases:

- empty password,
- whitespace-only password,
- common passwords such as `password`, `123456`, or `letmein`,
- repeated single-character passwords,
- passwords shorter than 8 characters,
- simple sequential ASCII strings,
- short passwords with very limited character variety,
- short passwords with a single character class,
- short passwords with very low unique-character variety.

Longer multiword passphrases such as this are not flagged by this check:

```text
correct horse battery staple
```

### Import passwords

The import page appears when all of these are true:

- Linux build,
- Host backend is active,
- at least one store exists,
- the configured host `pass` command supports `pass import`.

You can choose:

- the target store,
- the importer name,
- an optional source file or folder,
- an optional store subfolder.

### Logs and setup helpers

Linux builds expose a log view with `F12`.

The **Logs** group can include:

- **Docs**, which opens the standalone docs page,
- **Open logs**,
- **Copy logs** in regular builds,
- a local app-menu install or uninstall action in setup-enabled builds.

## Recipient And Key Workflows

For store-level key changes:

1. Open **Password Stores** in Preferences.
2. Open the target store's **Store keys** page.
3. Add or remove recipients.
4. Optionally generate a private key, import one, or attach a hardware key.
5. Save changes.

On Linux, if the Integrated backend needs a private key unlocked to re-encrypt entries or sign the Git commit, Keycord prompts for it. If the signing unlock dialog is dismissed, the save can continue without a Git signature.

## Keyboard Shortcuts

### Pass files

| Shortcut | Action |
| --- | --- |
| `Ctrl+N` | Open a new item |
| `Ctrl+S` | Save current page, or sync from the home page when available |
| `Ctrl+Shift+R` | Open raw text |
| `Ctrl+Shift+C` | Copy password |
| `Ctrl+Shift+U` | Copy username |
| `Ctrl+Shift+T` | Copy OTP |
| `Ctrl+Shift+A` | Apply template |
| `Ctrl+Shift+F` | Add field |
| `Ctrl+Shift+O` | Add OTP field |
| `Ctrl+Shift+P` | Password options |
| `Ctrl+Shift+K` | Clean pass file |
| `Ctrl+Shift+G` | Generate password |
| `Ctrl+Z` | Undo or revert changes |

### List and navigation

| Shortcut | Action |
| --- | --- |
| `Ctrl+F` | Toggle find |
| `Ctrl+C` | Copy selected item's password |
| `F2` | Rename selected pass file |
| `Ctrl+M` | Move selected pass file |
| `Delete` | Delete selected pass file |
| `Ctrl+H` | Show hidden and duplicate entries |
| `Ctrl+Shift+S` | Sync stores |
| `F5` | Refresh current list context |
| `Escape` | Go back |
| `Home` | Go home |
| `Ctrl+Shift+N` | Add or create store |
| `Ctrl+G` | Open Git tools |

### General

| Shortcut | Action |
| --- | --- |
| `Ctrl+,` | Open preferences |
| `Ctrl+Shift+D` | Open docs |
| `Ctrl+T` | Open tools |
| `Ctrl+?` | Show shortcuts |
| `F1` | About |
| `F12` | Open logs |

## Next Reading

- [Search Guide](search.md)
- [Permissions and Backends](permissions-and-backends.md)
- [Use Cases](use-cases.md)
