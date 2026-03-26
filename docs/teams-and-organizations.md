# Teams, Workgroups, And Organizations

Use shared stores for shared credentials. Keep personal secrets in personal stores.

## Shared-Store Model

Default model:

- use one or more dedicated shared stores
- keep each shared store on Git
- put every member on the recipient list
- keep paths and fields consistent

Example labels:

```text
shared/github/admin
shared/vpn/helpdesk
infra/aws/prod/root
infra/k8s/staging/admin
oncall/pagerduty
```

## Store Layout

Use one shared store when:

- most members need the same secrets
- the team is small
- the recipient list is stable

Use multiple stores when:

- recipient lists differ
- production access is narrower than staging or internal access
- teams onboard and offboard independently

Example split:

```text
~/stores/engineering
~/stores/support
~/stores/finance
~/stores/production-breakglass
```

## Backend Choice

- on Linux, use `Host` to restore a store from a Git URL or use `pass import`
- use `Integrated` for normal editing and app-managed key workflows
- on Linux Flatpak, remote Git sync and other host-driven features still need host access

See [Permissions and Backends](permissions-and-backends.md) for the full matrix.

## New Shared Store

### Create a store

1. choose an empty folder in **Password Stores**
2. open **Store keys**
3. add the initial recipients
4. save the recipients
5. confirm that members can decrypt
6. add a Git remote if the store will be shared through Git

### Restore an existing store

1. on Linux, switch to `Host` if needed
2. use **Restore password store**
3. choose the destination folder
4. enter the repository URL
5. open **Store keys**
6. verify the recipients

## Temporary Bootstrap Key

Use this when you are creating a new shared store and nobody else has a key in it yet.

Requirements:

- a password-protected software key
- a secure way to share the armored private key and its passphrase

Steps:

1. create the shared store
2. on **Store keys**, generate a temporary password-protected private key
3. keep that key selected and save the store
4. sync the store if it is Git-backed
5. copy the armored private key from the key row
6. share the key and its passphrase through a secure channel
7. each member clones or restores the store
8. each member imports the temporary key with **Import private key from clipboard** or **Import private key**
9. each member confirms that decryption works
10. each member generates or adds their own long-term key
11. each member selects their own key and saves the store
12. after all members have working long-term keys, remove the temporary key from the recipient list and sync
13. remove the temporary key file from Keycord

Limits:

- use this only for password-protected software keys
- the copied export is still a private key
- do not remove the temporary key before all members have confirmed access

## Daily Workflow

Before editing:

1. open the shared store
2. check Git status
3. sync first if remotes are configured

While editing:

- keep labels consistent
- keep field names consistent
- use structured fields where possible

Suggested fields:

```text
username:
email:
url:
owner:
environment:
notes:
```

After editing:

1. save the entry
2. sync from a clean repo
3. if the repo is dirty or detached, fix it on the host first

## Onboarding

When adding a member:

1. choose the key type: existing key, new password-protected key, or hardware key
2. add the recipient on **Store keys**
3. save the store recipients
4. have the member restore or open the store
5. confirm that decryption works

During the initial bootstrap phase, a member can import the temporary bootstrap key first and add their own long-term key afterward.

## Offboarding

When removing a member:

1. remove their recipient
2. save the store recipients
3. sync the store
4. rotate sensitive credentials if needed

Removing a recipient does not invalidate secrets the person already knows. Treat temporary bootstrap keys the same way.

## Conventions

### Paths

Pick one pattern and keep it stable:

```text
team/service/account
environment/service/account
department/tool/role
```

### Fields

Keep field names stable:

- `username`
- `email`
- `url`
- `owner`
- `environment`
- `notes`

Keycord normalizes `user`, `login`, and `username` into the same search field. Other fields are used as written.

### Store Boundaries

Split stores when:

- recipient lists differ
- sensitivity differs
- review or approval rules differ

## Review And Audits

Useful searches:

```text
find weak password
find otp
find url contains admin
find email is $username
reg:(?i)^prod/.+root$
```

Useful tools:

- **Find weak passwords**
- **Browse field values**

## High-Trust Stores

Keycord can require all selected managed keys for a store.

Use this only if you accept the tradeoff:

- it is Keycord-specific
- other `pass` apps cannot read those items

## Linux And Flatpak

For Flatpak:

- on Linux Flatpak, host access is needed for restore-from-Git and remote Git sync
- smartcard access is needed for hardware keys
- the Integrated backend still works without those permissions for local operations

On Linux, host private-key sync is also available. Read the risks first in [Permissions and Backends](permissions-and-backends.md).

## Rollout

1. start with one shared non-production store
2. agree on path and field conventions
3. confirm that every member can decrypt and sync
4. add weak-password and OTP reviews
5. split into more stores only when recipient or sensitivity boundaries require it

## Related Reading

- [Use Cases](use-cases.md)
- [Permissions and Backends](permissions-and-backends.md)
- [Workflows](workflows.md)
