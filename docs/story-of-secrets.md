# Story of Secrets

This is a code-oriented walkthrough of how Keycord moves secret data from store creation to password copy.

## Story 1: A Store Is Born

The store flow starts in [src/store/management.rs](../src/store/management.rs). When the user picks a folder, Keycord checks whether it is empty.

If the folder is not empty, Keycord treats it as an existing store and opens the store-key editor.

If the folder is empty, Keycord opens the create-store version of the store-key page through [src/store/recipients_page/mod.rs](../src/store/recipients_page/mod.rs). Create mode immediately queues an autosave, but the save only becomes real once there is at least one recipient.

The recipient page keeps an in-memory list of selected recipients. Before saving, [src/store/recipients.rs](../src/store/recipients.rs) splits that list into:

- standard recipients that belong in `.gpg-id`
- FIDO2 recipients that belong in `.fido-id`

The actual save path lives in [src/store/recipients_page/save.rs](../src/store/recipients_page/save.rs) and [src/backend/integrated/store.rs](../src/backend/integrated/store.rs):

1. Keycord gathers the current recipients and the current private-key requirement.
2. `save_store_recipients` ensures the store directory exists.
3. It decrypts every existing entry first.
4. It writes updated recipient files.
5. It re-encrypts every entry with the new policy.
6. If the store is brand new, it can initialize Git too.

Two details matter here.

First, recipient files are transactional. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) writes the new `.gpg-id` and FIDO2 sidecar, runs the reencryption closure, and restores the old files if reencryption fails.

Second, recipients are inherited per path. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) resolves an entry's recipients by walking upward until it finds the nearest `.gpg-id`. So the "story of a secret" is really "find the closest recipient file, then use that policy."

## Story 2: A Secret Is Written

The new-item dialog is built in [src/password/new_item.rs](../src/password/new_item.rs). It picks a store root and a pass-file label such as `team/service`.

When the editor opens in [src/password/page/mod.rs](../src/password/page/mod.rs), Keycord seeds the new file from the "new password template" in Preferences. [src/password/file/compose.rs](../src/password/file/compose.rs) turns that template into initial plaintext where:

- the first line is the password slot
- later lines are structured fields such as `username:` or `url:`

While the user edits, [src/password/page/editor.rs](../src/password/page/editor.rs) and [src/password/file/compose.rs](../src/password/file/compose.rs) keep rebuilding the pass-file text in memory. Keycord does not encrypt field-by-field. It always composes one plaintext pass file first, then encrypts the whole thing.

On save, [src/password/page/mod.rs](../src/password/page/mod.rs) calls into [src/backend/mod.rs](../src/backend/mod.rs), which dispatches to the active backend. The integrated save path is [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs).

That save path does four important things:

1. It resolves the final file path for the label.
2. It loads the crypto context from the nearest recipient files.
3. It encrypts the plaintext according to the store policy.
4. It writes the ciphertext to disk.

The file extension is part of the policy. [src/backend/integrated/shared/paths.rs](../src/backend/integrated/shared/paths.rs) and [src/password/entry_files.rs](../src/password/entry_files.rs) choose:

- `.gpg` for standard recipient stores
- `.keycord` for FIDO2 recipient stores

Existing legacy files are still respected, so a FIDO2-backed entry can continue to read an older `.gpg` file until it is rewritten.

## Story 3: Password-Protected Key

This is the normal managed-key path.

The UI for generating the key is in [src/store/recipients_page/generate.rs](../src/store/recipients_page/generate.rs). The real key generation happens in [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs):

1. `generate_ripasso_private_key` creates a Sequoia certificate with a required passphrase.
2. It serializes the secret key material.
3. It immediately imports that material back into Keycord's managed-key storage.

Imports use the same storage module. The important rule is enforced in [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs): Keycord refuses to keep an unprotected software private key. Imported software keys must already be password protected.

Unlocking is session-based. [src/private_key/unlock.rs](../src/private_key/unlock.rs) collects the passphrase, then [src/backend/integrated/keys/store.rs](../src/backend/integrated/keys/store.rs) decrypts the stored key and caches the unlocked certificate in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs).

When an entry is read, [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs) builds a candidate list through [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs):

- recipients for the entry
- the selected "own" fingerprint, if configured
- every imported managed key

If the needed key is still locked, the read fails with a locked-key error. The copy and open flows catch that error and reroute back into the unlock dialog through [src/clipboard.rs](../src/clipboard.rs) or [src/private_key/unlock.rs](../src/private_key/unlock.rs).

For encryption, [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) builds a normal OpenPGP recipient list and encrypts the whole pass file once.

## Story 4: Require All Keys

This option starts in the store-key UI. [src/store/recipients_page/list.rs](../src/store/recipients_page/list.rs) exposes the "require all" toggle when the store is using normal managed keys.

Saving that option does not create a new file. It adds metadata to `.gpg-id`. [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs) writes:

```text
# keycord-private-key-requirement=all
```

That one comment changes the whole read and write path.

On write, [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) switches from "any selected key may open this" to layered encryption:

1. Encrypt the plaintext for the innermost required recipient.
2. Wrap that ciphertext in a `keycord-require-all-private-keys-v1` layer.
3. Encrypt that wrapped value for the next recipient.
4. Repeat until every required key has added a layer.

On read, the same module reverses the process one recipient at a time. If even one required key is missing, incompatible, or still locked, the secret does not open.

There is one extra rule hidden in [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs): a FIDO2-only store with more than one FIDO2 recipient is treated as `AllManagedKeys` even if the comment is absent. In other words, "all keys required" is explicit for normal keys and implicit for multi-key FIDO2-only stores.

## Story 5: FIDO2 Security Key

The FIDO2 add flow lives in [src/store/recipients_page/import.rs](../src/store/recipients_page/import.rs), but the real work happens in [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs).

When the user adds a FIDO2 security key:

1. Keycord enrolls an `hmac-secret` credential against the Keycord RP ID.
2. It derives a stable recipient id from the credential id.
3. It stores a temporary enrollment record in memory.
4. It returns a recipient string such as `keycord-fido2-recipient-v1=...`.

The recipient string format is defined in [src/fido2_recipient.rs](../src/fido2_recipient.rs). The recipient itself is saved to `.fido-id`, not `.gpg-id`.

The temporary enrollment cache in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs) is important. It lets the first save use the just-created FIDO2 secret material without forcing the user to immediately re-derive it from the device again. After a successful store-recipient save, [src/backend/integrated/store.rs](../src/backend/integrated/store.rs) clears that pending enrollment state.

FIDO2 entry encryption is different from standard OpenPGP entry encryption.

For the common any-key path, [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs) and [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) create an any-managed bundle:

- a random data-encryption key encrypts the pass-file payload once
- each FIDO2 recipient gets its own wrapped copy of that key
- standard OpenPGP recipients can also get a wrapped copy of the same key

That means the payload is encrypted once, but multiple recipient wrappers point at it.

For rewrites, [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) tries to preserve existing wrapped recipients when possible. That is why adding or removing one FIDO2 key does not always force a full rebuild of every FIDO2 wrapper.

For the all-keys-required path, FIDO2 uses direct required layers instead of the any-managed bundle.

Unlocking is also session-based. [src/private_key/unlock.rs](../src/private_key/unlock.rs) can ask for a FIDO2 PIN, then [src/backend/integrated/keys/fido2.rs](../src/backend/integrated/keys/fido2.rs) validates the device and caches the PIN in [src/backend/integrated/keys/cache.rs](../src/backend/integrated/keys/cache.rs).

The extra guidance dialog in [src/store/recipients_page/guide.rs](../src/store/recipients_page/guide.rs) exists for a real reason: when you add another FIDO2 recipient to an existing FIDO2 store, Keycord still has to decrypt the old entries before it can re-wrap them for the new set of keys. That is why it may ask for a security key that already works with the store.

## Story 6: A Secret Is Opened

Opening a password entry starts in [src/password/page/mod.rs](../src/password/page/mod.rs). The page shows a loading state and then calls `read_password_entry_with_progress`.

The integrated read path in [src/backend/integrated/entries.rs](../src/backend/integrated/entries.rs) branches by private-key requirement:

- `AnyManagedKey`: try candidates until one decrypts
- `AllManagedKeys`: require every selected key in order

The crypto context comes from [src/backend/integrated/shared/crypto.rs](../src/backend/integrated/shared/crypto.rs). The candidate list and recipient metadata come from [src/backend/integrated/shared/recipients.rs](../src/backend/integrated/shared/recipients.rs).

If the entry opens, the plaintext pass file goes back into the structured editor.

If the key is locked, Keycord surfaces a typed error from [src/backend/errors.rs](../src/backend/errors.rs), and the UI can prompt for the missing unlock step instead of just failing.

## Story 7: Copying the Password

The copy button on each password row is wired in [src/password/list/row.rs](../src/password/list/row.rs). It calls [src/clipboard.rs](../src/clipboard.rs).

From there the story is short:

1. If the integrated backend is active, Keycord reads only the first line of the entry through `read_password_line`.
2. If the read fails because the key is locked, Keycord resolves the preferred key and shows the unlock dialog.
3. If the read succeeds, Keycord writes the first line to the system clipboard and shows button feedback.

The important detail is that copy is still a decrypt operation. The password is not cached as ready-to-copy plaintext somewhere else in the app. Keycord re-enters the same read path, takes the first line, and hands that text to the clipboard.

If the Host backend is active, [src/clipboard.rs](../src/clipboard.rs) takes a different route and shells out to `pass -c` instead. The rest of this guide follows the integrated path because that is where store-key management, layered encryption, and FIDO2 behavior live.
