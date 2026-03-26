# Use Cases

Practical examples and short tutorials.

## Personal Store

Setup:

- backend: `Integrated`
- store count: `1`
- typical store: `~/.password-store`

Common workflow:

1. add or confirm the store in Preferences
2. create entries such as `personal/github` or `personal/bank`
3. keep the default template with username, email, and URL
4. add OTP where needed
5. review with search and tools

Common queries:

```text
find url contains github
find otp
find weak password
```

## Personal And Work Stores

Setup:

- backend: `Integrated` for normal use
- on Linux, switch to `Host` only for restore-from-Git or `pass import`
- store count: `2+`

Example:

```text
~/.password-store
~/work-password-store
```

Common workflow:

1. add both stores
2. keep labels consistent
3. search across both stores
4. use the field-value browser for repeated usernames, domains, or URLs

Example labels:

```text
personal/github
work/github
work/vpn
```

Common queries:

```text
github
find user alice
reg:(?i)^work/.+vpn$
```

## Shared Team Store

For a deeper guide, including the temporary bootstrap-key pattern, see [Teams and Organizations](teams-and-organizations.md).

Setup:

- backend: `Host` on Linux for initial restore, either backend after that
- store count: usually `1` shared store
- one recipient per team member

Common workflow:

1. restore the store from Git
2. open **Store keys** and confirm recipients
3. add or remove recipients as membership changes
4. check Git status before sync
5. sync only from a clean repo

Common queries:

```text
find url contains admin
find weak password
find otp AND url contains company.com
```

## High-Trust Store

Setup:

- backend: `Integrated`
- dedicated store or sub-store
- multiple managed keys selected

Common workflow:

1. open **Store keys**
2. add the required recipients
3. enable **Require all selected keys**
4. save the store recipients

Limit:

- this is Keycord-specific
- other `pass` apps cannot read those items

Typical use:

- break-glass credentials
- production root credentials
- multi-party access control

## DevOps And Admin Work

Setup:

- backend: `Integrated` for normal use
- `Host` where restore or import is needed
- multiple stores or strict path conventions

Example labels:

```text
prod/aws/root
prod/k8s/admin
staging/vpn
shared/oncall/github
```

Common workflow:

1. use structured labels and fields
2. run weak-password and OTP reviews
3. rotate recipients through **Store keys**
4. restore stores from Git when rebuilding workstations
5. generate or import long-term keys for each admin

Common queries:

```text
find weak password
find url contains github
find email is $username
reg:(?i)^prod/.+vpn$
find url contains internal.company
find otp AND user admin
```

## Quick Rule

- one personal store: start with `Integrated`
- personal + work separation: use multiple stores
- shared team store: focus on recipients and Git state
- high-trust store: use layered encryption only if you accept the compatibility loss
- admin-heavy setup: use path conventions, structured fields, and the tools page

## Next

- [Getting Started](getting-started.md)
- [Search Guide](search.md)
- [Permissions and Backends](permissions-and-backends.md)
- [Teams and Organizations](teams-and-organizations.md)
