#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 BUILD_DIR VARIANT" >&2
  exit 1
fi

build_dir="$1"
variant="$2"
script_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
source_root="$(cd -- "$script_dir/.." && pwd)"

case "$variant" in
  developer|release|translation)
    ;;
  *)
    echo "Unsupported Meson Cargo wrapper profile: $variant" >&2
    exit 1
    ;;
esac

setup_args=(
  setup
  "$build_dir"
  "$source_root"
  "-Dcargo_variant=$variant"
)

if [[ -d "$build_dir/meson-info" ]]; then
  setup_args=(
    setup
    "$build_dir"
    "$source_root"
    --reconfigure
    "-Dcargo_variant=$variant"
  )
fi

meson "${setup_args[@]}"
meson compile -C "$build_dir"
