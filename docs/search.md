# Search Guide

Keycord supports three search modes:

- plain label search,
- regex search with `reg`,
- structured field-aware search with `find`.

Malformed `find` or `reg` queries do not fall back to plain search.

## Quick Reference

| Mode | What it searches | Example |
| --- | --- | --- |
| Plain text | Entry labels only | `github` |
| `reg` | Labels plus indexed field corpus | `reg:(?i)^work/.+github$` |
| `find` | Structured fields and search predicates | `find url contains github` |

## What Counts As Searchable Data

### Plain search

Plain search only checks the visible label, such as:

```text
work/alice/github
```

It does not search field values.

### Regex search

`reg` queries use regular expressions and match:

- labels,
- plus a searchable field corpus built from indexed structured fields.

That means a regex can match `email: alice@example.com` even when the label itself does not contain that value.

### Structured `find` search

`find` queries work on structured search fields:

- `username` plus its aliases `user` and `login`,
- any other `key: value` field in the pass file,
- the `otp` predicate,
- the `weak password` predicate.

Important limits:

- `otpauth` is not treated as a normal searchable field.
- Use `find otp` instead of trying to search `otpauth` directly.
- Lines without a structured `key: value` form are not field-searchable.

## Plain Search

Use plain search when you only need to find by path or name:

```text
github
vpn
work/alice
```

This behaves like a label filter.

## Regex Search With `reg`

Regex search starts with `reg:` or `reg `.

Examples:

```text
reg:(?i)^work/.+github$
reg team/.+service
reg:(?i)email:\s+alice@example\.com
```

Notes:

- `(?i)` works for case-insensitive regex.
- A malformed regex such as `reg:[` is invalid and returns no matches.

## Structured Search With `find`

Structured search starts with `find:` or `find `.

Examples:

```text
find user alice
find url contains github
find email is $username
```

### Search Field Names

Keycord normalizes these username aliases to the same field:

- `username`
- `user`
- `login`

Everything else uses the pass-file field key, case-insensitively:

```text
find email contains example.com
find url is https://example.com
find "security question" is "first pet"
```

## Operators

### Contains

These forms are equivalent:

```text
find username=noob
find username~=noob
find username contains noob
find user noob
```

### Does not contain

```text
find url!~gitlab
find url does not contain gitlab
```

### Exact match

```text
find username==alice
find username is alice
```

### Exact mismatch

```text
find username!=alice
find username is not alice
```

### Regex match inside `find`

```text
find user matches '^Alice$'
find user regex '^Alice$'
```

### Regex mismatch inside `find`

```text
find user does not match '^Alice$'
find url not regex 'gitlab|github'
```

## Field References

You can compare one field to another with `$field_name`, but only for exact comparisons.

Valid:

```text
find email is $username
find email is not $user
find "backup email" == $"security question"
```

Invalid:

```text
find email contains $username
find email ~= $username
find user regex $email
```

## Boolean Logic And Precedence

Keycord supports:

- `NOT` or `!`
- `AND`, `&&`, or `WITH`
- `OR` or `||`
- parentheses

Precedence is:

1. `NOT`
2. `AND` / `WITH`
3. `OR`

Examples:

```text
find username=noob AND url=gitlab OR email==alice@example.com
find (username=noob OR url=gitlab) AND email==alice@example.com
find !username~=alice
find not email is $username
```

## Special Predicates

### OTP predicate

Matches entries that contain OTP data:

```text
find otp
find otp AND user alice
```

Do not search `otpauth` as a normal field. That is invalid:

```text
find otpauth contains totp
```

### Weak password predicate

Matches entries whose first password line fails Keycord's basic checks:

```text
find weak password
find weak
find weak password AND username==alice
find not weak password
```

## Quoting And Escaping

Quote values or field names that contain spaces or reserved words:

```text
find "security question" is "first pet"
find notes matches 'Personal (OR|AND) Work'
find "matches" is "keyword field"
```

Inside quoted values:

- escape `"` as `\"` inside double quotes,
- escape `'` as `\'` inside single quotes,
- escape `\` as `\\`.

Examples:

```text
find:notes=="Personal OR Work \"vault\""
find:notes=='Personal OR Work \'vault\''
```

## Examples From Simple To Advanced

### Simple label searches

```text
github
personal/bank
vpn
```

### Simple structured searches

```text
find user alice
find email contains example.com
find url is https://github.com/login
```

### Multi-condition searches

```text
find user alice AND url contains github
find weak password AND url contains gitlab
find otp AND email is $username
```

### Regex-heavy searches

```text
reg:(?i)^work/.+github$
find user matches '^(alice|bob)$'
find notes not regex 'deprecated|legacy'
```

### Exact field comparison

```text
find email is $username
find email is not $username
```

### Audit-style searches

```text
find weak password OR otp
find (user alice OR user bob) AND url contains admin
find url contains github AND email contains company.com
```

## Common Invalid Queries

These return no results until corrected:

```text
find user
find username=
find url does not
find user matches
find email contains $username
find otpauth contains totp
reg:[
```

## Tips

- Use plain search for names and paths.
- Use `find` for field-aware searches.
- Use `reg` for regex across labels and indexed fields.
- Quote field names with spaces.
- Use `$username` comparisons for field consistency checks.

## Next Reading

- [Getting Started](getting-started.md)
- [Workflows](workflows.md)
- [Use Cases](use-cases.md)
