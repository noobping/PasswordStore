pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
sources = meson.project_build_root() / 'passwordstore' / 'src'
targets = meson.project_build_root() / 'passwordstore' / 'target'

cargo_bin  = find_program('cargo')
cargo_opt  = [ '--manifest-path', meson.project_source_root() / 'passwordstore' / 'Cargo.toml' ]
cargo_opt += [ '--target-dir', targets ]
cargo_env  = [ 'CARGO_HOME=' + meson.project_build_root()  / 'cargo-home' ]

if get_option('buildtype') == 'release'
  cargo_opt += [ '--release' ]
  rust_target = 'release'
else
  rust_target = 'debug'
endif

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    'env', cargo_env,
    cargo_bin, 'build',
    cargo_opt, '&&', 'cp', targets / rust_target / meson.project_name(), '@OUTPUT@',
  ]
)

subdir('src')
