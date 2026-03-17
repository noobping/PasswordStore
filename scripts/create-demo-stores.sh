#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Create a few self-contained demo pass stores for Keycord.

Usage:
  scripts/create-demo-stores.sh [--force] [target-dir]

Defaults:
  target-dir: /tmp/keycord-demo-stores

The script creates:
  - a dedicated GNUPGHOME under the target directory
  - three demo OpenPGP keys
  - three password stores with .gpg-id files and encrypted entries
  - armored key exports and a small README with usage notes

It does not touch your normal ~/.gnupg directory.
EOF
}

FORCE=0
TARGET_ROOT="/tmp/keycord-demo-stores"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [[ "$TARGET_ROOT" != "/tmp/keycord-demo-stores" ]]; then
        echo "Only one target directory can be provided." >&2
        usage >&2
        exit 1
      fi
      TARGET_ROOT="$1"
      shift
      ;;
  esac
done

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "Required command not found: $name" >&2
    exit 1
  fi
}

require_command gpg
require_command git

if [[ -e "$TARGET_ROOT" ]]; then
  if [[ "$FORCE" -eq 1 ]]; then
    rm -rf "$TARGET_ROOT"
  elif find "$TARGET_ROOT" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
    echo "Target directory already exists and is not empty: $TARGET_ROOT" >&2
    echo "Re-run with --force to replace it." >&2
    exit 1
  fi
fi

GNUPGHOME="$TARGET_ROOT/gnupg"
STORES_DIR="$TARGET_ROOT/stores"
EXPORTS_DIR="$TARGET_ROOT/exports"
README_PATH="$TARGET_ROOT/README.txt"
ENV_PATH="$TARGET_ROOT/demo-env.sh"

mkdir -p "$GNUPGHOME" "$STORES_DIR" "$EXPORTS_DIR"
chmod 700 "$GNUPGHOME"
export GNUPGHOME

cat >"$GNUPGHOME/gpg.conf" <<'EOF'
pinentry-mode loopback
batch
quiet
EOF

declare -A FINGERPRINTS
declare -A STORE_PATHS

create_key() {
  local slug="$1"
  local name="$2"
  local email="$3"

  gpg --batch --generate-key <<EOF >/dev/null
Key-Type: RSA
Key-Length: 3072
Subkey-Type: RSA
Subkey-Length: 3072
Name-Real: $name
Name-Email: $email
Expire-Date: 0
%no-protection
%commit
EOF

  local fingerprint
  fingerprint="$(
    gpg --batch --with-colons --list-secret-keys "$email" \
      | awk -F: '/^fpr:/ { print $10; exit }'
  )"

  if [[ -z "$fingerprint" ]]; then
    echo "Failed to resolve fingerprint for $email" >&2
    exit 1
  fi

  FINGERPRINTS["$slug"]="$fingerprint"
  gpg --batch --yes --armor --export-secret-keys "$fingerprint" >"$EXPORTS_DIR/$slug.secret.asc"
  gpg --batch --yes --armor --export "$fingerprint" >"$EXPORTS_DIR/$slug.public.asc"
}

create_store() {
  local slug="$1"
  shift
  local store_path="$STORES_DIR/$slug"
  mkdir -p "$store_path"
  printf '%s\n' "$@" >"$store_path/.gpg-id"
  STORE_PATHS["$slug"]="$store_path"
}

write_entry() {
  local store_path="$1"
  local label="$2"
  local entry_path="$store_path/$label.gpg"
  local recipients=()

  mkdir -p "$(dirname "$entry_path")"
  while IFS= read -r recipient; do
    [[ -n "$recipient" ]] || continue
    recipients+=("-r" "$recipient")
  done <"$store_path/.gpg-id"

  gpg --batch --yes --trust-model always --encrypt "${recipients[@]}" -o "$entry_path"
}

init_store_git() {
  local store_path="$1"
  git -C "$store_path" init -q
  git -C "$store_path" config user.name "Keycord Demo"
  git -C "$store_path" config user.email "demo@example.test"
  git -C "$store_path" add .
  git -C "$store_path" commit -qm "Initial demo store"
}

create_key "nick" "Nick Demo" "nick@example.test"
create_key "alice" "Alice Demo" "alice@example.test"
create_key "ops" "Ops Demo" "ops@example.test"

create_store \
  "personal-store" \
  "${FINGERPRINTS[nick]}"

create_store \
  "team-store" \
  "${FINGERPRINTS[nick]}" \
  "${FINGERPRINTS[alice]}"

create_store \
  "ops-store" \
  "${FINGERPRINTS[ops]}" \
  "${FINGERPRINTS[nick]}"

write_entry "${STORE_PATHS[personal-store]}" "email/proton" <<'EOF'
correct horse battery staple 2026!
username: nick
email: nick@example.test
url: https://account.proton.me
notes: personal or work
EOF

write_entry "${STORE_PATHS[personal-store]}" "social/mastodon" <<'EOF'
password123
user: nick
url: https://mastodon.social/@nick
security question: first pet
otpauth: otpauth://totp/Keycord:nick?secret=JBSWY3DPEHPK3PXP&issuer=Keycord
EOF

write_entry "${STORE_PATHS[team-store]}" "servers/prod-app" <<'EOF'
Tr0ub4dor&3
login: deploy
url: https://prod.demo.invalid
environment: production
notes: shared with alice
EOF

write_entry "${STORE_PATHS[team-store]}" "ci/github-actions" <<'EOF'
123456
username: ci-bot
url: https://github.com/noobping/keycord
notes: matches '^ci'
EOF

write_entry "${STORE_PATHS[ops-store]}" "vpn/lab" <<'EOF'
lab-vpn-demo
username: ops
url: https://vpn.demo.invalid
region: eu-west
EOF

write_entry "${STORE_PATHS[ops-store]}" "databases/staging" <<'EOF'

username: staging-admin
url: https://db.demo.invalid
notes: empty password line on purpose
EOF

init_store_git "${STORE_PATHS[personal-store]}"
init_store_git "${STORE_PATHS[team-store]}"
init_store_git "${STORE_PATHS[ops-store]}"

cat >"$ENV_PATH" <<EOF
export GNUPGHOME='$GNUPGHOME'
export KEYCORD_DEMO_ROOT='$TARGET_ROOT'
EOF

cat >"$README_PATH" <<EOF
Keycord demo stores created in:
  $TARGET_ROOT

Before testing host-backend decryption, use:
  source "$ENV_PATH"

Then add these stores in Keycord Preferences:
  ${STORE_PATHS[personal-store]}
  ${STORE_PATHS[team-store]}
  ${STORE_PATHS[ops-store]}

Armored demo keys are in:
  $EXPORTS_DIR

Useful example queries:
  find weak password
  find user nick with weak password
  find url matches '^https://github'
  find "security question" is "first pet"
EOF

cat <<EOF
Created demo stores in:
  $TARGET_ROOT

Store paths:
  ${STORE_PATHS[personal-store]}
  ${STORE_PATHS[team-store]}
  ${STORE_PATHS[ops-store]}

To use the demo GNUPG home for host-backend testing:
  source "$ENV_PATH"
EOF
