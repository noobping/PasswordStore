#!/usr/bin/env bash
set -euo pipefail

source_root=""
build_root=""
cargo_bin=""
variant=""
output=""
cargo_features=""
cargo_cmd=()

while (($# > 0)); do
  case "$1" in
    --source-root)
      source_root="$2"
      shift 2
      ;;
    --build-root)
      build_root="$2"
      shift 2
      ;;
    --cargo)
      cargo_bin="$2"
      shift 2
      ;;
    --variant)
      variant="$2"
      shift 2
      ;;
    --features)
      cargo_features="$2"
      shift 2
      ;;
    --output)
      output="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$source_root" || -z "$build_root" || -z "$variant" || -z "$output" ]]; then
  echo "Missing required arguments for the Meson Cargo wrapper." >&2
  exit 1
fi

resolve_cargo() {
  if [[ -n "$cargo_bin" ]]; then
    cargo_cmd=("$cargo_bin")
    return
  fi

  if command -v cargo >/dev/null 2>&1; then
    cargo_cmd=("cargo")
    return
  fi

  if command -v flatpak-spawn >/dev/null 2>&1; then
    local host_cargo
    host_cargo="$(flatpak-spawn --host which cargo 2>/dev/null || true)"
    if [[ -n "$host_cargo" ]]; then
      cargo_cmd=("flatpak-spawn" "--host" "$host_cargo")
      return
    fi

    if flatpak-spawn --host which toolbox >/dev/null 2>&1; then
      if flatpak-spawn --host toolbox run which cargo >/dev/null 2>&1; then
        cargo_cmd=("flatpak-spawn" "--host" "toolbox" "run" "cargo")
        return
      fi
    fi
  fi

  echo "Unable to find cargo in PATH, through flatpak-spawn --host, or through toolbox." >&2
  exit 1
}

cargo_args=(build)
artifact_profile="debug"
cargo_summary="cargo build"

case "$variant" in
  developer)
    ;;
  release|translation)
    cargo_args+=(--release)
    artifact_profile="release"
    cargo_summary="cargo build --release"
    ;;
  *)
    echo "Unsupported cargo variant: $variant" >&2
    exit 1
    ;;
esac

if [[ -n "$cargo_features" ]]; then
  cargo_args+=(--features "$cargo_features")
  cargo_summary+=" --features $cargo_features"
fi

resolve_cargo
export CARGO_TARGET_DIR="$build_root/cargo-target"
mkdir -p "$CARGO_TARGET_DIR"
mkdir -p "$(dirname "$output")"

echo "Meson Cargo wrapper: $variant -> $cargo_summary"
(
  cd "$source_root"
  "${cargo_cmd[@]}" "${cargo_args[@]}"
)

artifact_path="$CARGO_TARGET_DIR/$artifact_profile/keycord"
if [[ ! -f "$artifact_path" ]]; then
  echo "Expected Cargo artifact not found: $artifact_path" >&2
  exit 1
fi

install -Dm755 "$artifact_path" "$output"
