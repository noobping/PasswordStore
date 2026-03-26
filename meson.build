project(
  'keycord',
  version: '0.9.2',
  meson_version: '>=1.1.0',
)

cargo_wrapper = find_program('scripts/meson-cargo-build.sh')
gnome = import('gnome')

cargo_variant = get_option('cargo_variant')
cargo_features = get_option('cargo_features')
project_binary = 'keycord'
cargo_command = 'cargo build'

if cargo_variant == 'release' or cargo_variant == 'translation'
  cargo_command = 'cargo build --release'
endif

message('Configured Meson wrapper variant "@0@" -> @1@'.format(cargo_variant, cargo_command))
if cargo_features != ''
  message('Configured Meson wrapper features "@0@"'.format(cargo_features))
endif

cargo_wrapper_args = [
  cargo_wrapper,
  '--source-root', meson.project_source_root(),
  '--build-root', meson.project_build_root(),
]

if cargo_features != ''
  cargo_wrapper_args += ['--features', cargo_features]
endif

custom_target(
  'keycord-default-build',
  output: project_binary,
  command: cargo_wrapper_args + [
    '--variant', cargo_variant,
    '--output', '@OUTPUT@',
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
      '--variant', variant,
      '--output', join_paths(meson.project_build_root(), 'artifacts', variant, project_binary),
    ],
  )
endforeach

install_data(
  'data/scalable/apps/io.github.noobping.keycord.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

install_data(
  'keycord.desktop',
  rename: 'io.github.noobping.keycord.desktop',
  install_dir: join_paths(get_option('datadir'), 'applications'),
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
