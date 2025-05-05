cargo_bin  = find_program('cargo')
cargo_opt  = [ '--manifest-path', meson.project_source_root() / 'passcore' / 'Cargo.toml' ]
cargo_opt += [ '--target-dir', meson.project_build_root()  / 'passcore' / 'src' ]
cargo_env  = [ 'CARGO_HOME=' + meson.project_build_root()  / 'cargo-home' ]

if get_option('buildtype') == 'release'
  cargo_opt += [ '--release' ]
  rust_target = 'release'
else
  rust_target = 'debug'
endif

# Where Cargo will drop its artefacts.  We keep them inside Meson’s build dir
# so “meson dist” stays tidy.
# build the static library
# Cargo’s output path for a staticlib crate is predictable:
libname = 'libpasscore.a'
libpath = meson.current_build_dir() / 'src' / rust_target / libname

libpasscore = custom_target(
  'passcore-staticlib',
  output          : libname,
  command         : [
    'env', cargo_env,
    cargo_bin, 'build',
    cargo_opt, '&&', 'cp', libpath, '@OUTPUT@',
  ],
  install         : true,
  install_dir     : get_option('libdir'),
  build_by_default: true,
  console         : true
)
