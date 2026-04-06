project(
  'keycord',
  version: '0.9.9',
  meson_version: '>=1.1.0',
)

cargo = find_program('cargo', required: false)
flatpak_spawn = find_program('flatpak-spawn', required: false)
shell = find_program('sh')
gnome = import('gnome')

cargo_variant = get_option('cargo_variant')
cargo_features = get_option('cargo_features')
docs = get_option('docs')
logging = get_option('logging')
audit = get_option('audit')
fidostore = get_option('fidostore')
fidokey = get_option('fidokey')
fidopin = get_option('fidopin')
smartcard = get_option('smartcard')
hardwarekey = get_option('hardwarekey')
project_binary = 'keycord'
cargo_command = 'cargo build'

effective_cargo_features = cargo_features
foreach feature_name, enabled : {
  'docs': docs,
  'logging': logging,
  'audit': audit,
  'fidostore': fidostore,
  'fidokey': fidokey,
  'fidopin': fidopin,
  'smartcard': smartcard,
  'hardwarekey': hardwarekey,
}
  if enabled
    if effective_cargo_features == ''
      effective_cargo_features = feature_name
    else
      effective_cargo_features += ' ' + feature_name
    endif
  endif
endforeach

if cargo_variant == 'release' or cargo_variant == 'translation'
  cargo_command = 'cargo build --release'
endif

message('Configured Meson wrapper variant "@0@" -> @1@'.format(cargo_variant, cargo_command))
if effective_cargo_features != ''
  message('Configured Meson wrapper features "@0@"'.format(effective_cargo_features))
endif

native_cargo = ''
flatpak_spawn_path = ''
if cargo.found()
  native_cargo = cargo.full_path()
  message('Configured Meson wrapper cargo source "native cargo" -> @0@'.format(native_cargo))
elif flatpak_spawn.found()
  flatpak_spawn_path = flatpak_spawn.full_path()
  message('Configured Meson wrapper cargo source "flatpak-spawn --host toolbox run cargo"')
else
  error('Neither cargo nor flatpak-spawn was found. Install cargo in the build environment or provide flatpak-spawn access to host toolbox.')
endif

if flatpak_spawn.found()
  flatpak_spawn_path = flatpak_spawn.full_path()
endif

cargo_wrapper_script = '''
set -eu

native_cargo=$1
flatpak_spawn=$2
source_root=$3
build_root=$4
variant=$5
features=$6
output=$7

profile_dir=debug

if [ "$variant" = "release" ] || [ "$variant" = "translation" ]; then
  profile_dir=release
fi

if [ -n "$native_cargo" ]; then
  set -- "$native_cargo" build "--manifest-path" "$source_root/Cargo.toml" "--target-dir" "$build_root/cargo-target" "--no-default-features"
else
  set -- "$flatpak_spawn" --host toolbox run cargo build "--manifest-path" "$source_root/Cargo.toml" "--target-dir" "$build_root/cargo-target" "--no-default-features"
fi

if [ "$profile_dir" = "release" ]; then
  set -- "$@" --release
fi

if [ -n "$features" ]; then
  set -- "$@" --features "$features"
fi

"$@"

mkdir -p "$(dirname "$output")"
cp "$build_root/cargo-target/$profile_dir/keycord" "$output"
chmod +x "$output"
'''

cargo_wrapper_args = [
  shell,
  '-c',
  cargo_wrapper_script,
  'meson-cargo-toolbox',
  native_cargo,
  flatpak_spawn_path,
  meson.project_source_root(),
  meson.project_build_root(),
]

custom_target(
  'keycord-default-build',
  output: project_binary,
  command: cargo_wrapper_args + [
    cargo_variant,
    effective_cargo_features,
    '@OUTPUT@',
  ],
  build_always_stale: true,
  build_by_default: true,
  console: true,
  install: true,
  install_dir: get_option('bindir'),
)

foreach variant : ['developer', 'release', 'translation']
  run_target(
    variant + '-build',
    command: cargo_wrapper_args + [
      variant,
      effective_cargo_features,
      join_paths(meson.project_build_root(), 'artifacts', variant, project_binary),
    ],
  )
endforeach

install_data(
  'data/scalable/apps/io.github.noobping.keycord.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

install_data(
  'data/scalable/apps/io.github.noobping.keycord-beta.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

install_data(
  'data/256x256/apps/io.github.noobping.keycord.png',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', '256x256', 'apps'),
)

install_data(
  'data/256x256/apps/io.github.noobping.keycord-beta.png',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', '256x256', 'apps'),
)

install_subdir(
  'data/symbolic/apps',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'symbolic'),
)

install_data(
  'keycord.desktop',
  rename: 'io.github.noobping.keycord.desktop',
  install_dir: join_paths(get_option('datadir'), 'applications'),
)

install_data(
  'keycord-search-provider.ini',
  rename: 'io.github.noobping.keycord.search-provider.ini',
  install_dir: join_paths(get_option('datadir'), 'gnome-shell', 'search-providers'),
)

install_data(
  'keycord-search-provider.service',
  rename: 'io.github.noobping.keycord.SearchProvider.service',
  install_dir: join_paths(get_option('datadir'), 'dbus-1', 'services'),
)

install_data(
  'data/gschema.xml',
  rename: 'io.github.noobping.keycord.gschema.xml',
  install_dir: join_paths(get_option('datadir'), 'glib-2.0', 'schemas'),
)

install_data(
  'data/metainfo.xml',
  rename: 'io.github.noobping.keycord.metainfo.xml',
  install_dir: join_paths(get_option('datadir'), 'metainfo'),
)

gnome.post_install(
  glib_compile_schemas: true,
)
