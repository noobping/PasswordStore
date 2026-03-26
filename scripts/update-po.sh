#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
po_dir="$repo_root/po"

domain="$(awk -F '"' '/^name = "/ { print $2; exit }' "$repo_root/Cargo.toml")"
version="$(awk -F '"' '/^version = "/ { print $2; exit }' "$repo_root/Cargo.toml")"
msgid_bugs_address="$(awk -F '"' '/^repository = "/ { print $2 "/issues"; exit }' "$repo_root/Cargo.toml")"
pot_path="$po_dir/$domain.pot"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

common_xgettext_args=(
  --from-code=UTF-8
  --package-name="$domain"
  --package-version="$version"
  --msgid-bugs-address="$msgid_bugs_address"
  --sort-by-file
  --add-comments=TRANSLATORS:
)

potfiles=()
while IFS= read -r line || [[ -n "$line" ]]; do
  line="${line%%#*}"
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  [[ -n "$line" ]] || continue
  potfiles+=("$line")
done < "$po_dir/POTFILES.in"

rust_files=()
gtkbuilder_files=()
gschema_files=()
metainfo_files=()

for rel_path in "${potfiles[@]}"; do
  case "$rel_path" in
    *.rs)
      rust_files+=("$rel_path")
      ;;
    *.ui)
      gtkbuilder_files+=("$rel_path")
      ;;
    *.gschema.xml|data/gschema.xml)
      gschema_files+=("$rel_path")
      ;;
    *.metainfo.xml|*.appdata.xml|data/metainfo.xml)
      metainfo_files+=("$rel_path")
      ;;
    *)
      echo "Unsupported translatable file in po/POTFILES.in: $rel_path" >&2
      exit 1
      ;;
  esac
done

parts=()

if ((${#rust_files[@]} > 0)); then
  rust_pot="$tmp_dir/rust.pot"
  (
    cd "$repo_root"
    xgettext \
      "${common_xgettext_args[@]}" \
      --language=Rust \
      --keyword=gettext \
      --keyword=ngettext:1,2 \
      --keyword=pgettext:1c,2 \
      --output="$rust_pot" \
      "${rust_files[@]}"
  )
  parts+=("$rust_pot")
fi

if ((${#gtkbuilder_files[@]} > 0)); then
  gtkbuilder_pot="$tmp_dir/gtkbuilder.pot"
  (
    cd "$repo_root"
    xgettext \
      "${common_xgettext_args[@]}" \
      --its=/usr/share/gettext/its/gtkbuilder.its \
      --output="$gtkbuilder_pot" \
      "${gtkbuilder_files[@]}"
  )
  parts+=("$gtkbuilder_pot")
fi

if ((${#gschema_files[@]} > 0)); then
  gschema_pot="$tmp_dir/gschema.pot"
  (
    cd "$repo_root"
    xgettext \
      "${common_xgettext_args[@]}" \
      --its=/usr/share/gettext/its/gschema.its \
      --output="$gschema_pot" \
      "${gschema_files[@]}"
  )
  parts+=("$gschema_pot")
fi

if ((${#metainfo_files[@]} > 0)); then
  metainfo_pot="$tmp_dir/metainfo.pot"
  (
    cd "$repo_root"
    xgettext \
      "${common_xgettext_args[@]}" \
      --its=/usr/share/gettext/its/metainfo.its \
      --output="$metainfo_pot" \
      "${metainfo_files[@]}"
  )
  parts+=("$metainfo_pot")
fi

if ((${#parts[@]} == 0)); then
  echo "No translatable files found." >&2
  exit 1
fi

(
  cd "$repo_root"
  msgcat --use-first --sort-by-file --output-file="$pot_path" "${parts[@]}"
)

while IFS= read -r locale || [[ -n "$locale" ]]; do
  locale="${locale%%#*}"
  locale="${locale#"${locale%%[![:space:]]*}"}"
  locale="${locale%"${locale##*[![:space:]]}"}"
  [[ -n "$locale" ]] || continue

  po_file="$po_dir/$locale.po"
  if [[ "$locale" == "en" ]]; then
    rm -f "$po_file"
    msginit --no-translator --locale="$locale" --input="$pot_path" --output-file="$po_file" >/dev/null
  elif [[ -f "$po_file" ]]; then
    msgmerge --update --backup=none --previous "$po_file" "$pot_path"
    msgattrib --no-obsolete --output-file="$po_file" "$po_file"
  else
    msginit --no-translator --locale="$locale" --input="$pot_path" --output-file="$po_file" >/dev/null
  fi

  msgfmt --check --output-file=/dev/null "$po_file"
done < "$po_dir/LINGUAS"
