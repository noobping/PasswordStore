datadir = get_option('prefix') / get_option('datadir')
bindir  = get_option('prefix') / get_option('bindir')

desktop_file = i18n.merge_file(
        input: 'io.github.noobping.PasswordStore.desktop.in',
       output: 'io.github.noobping.PasswordStore.desktop',
         type: 'desktop',
       po_dir: '../po',
      install: true,
  install_dir: datadir / 'applications'
)

desktop_file_search = i18n.merge_file(
        input: 'io.github.noobping.PasswordStore.SearchProvider.desktop.in',
       output: 'io.github.noobping.PasswordStore.SearchProvider.desktop',
         type: 'desktop',
       po_dir: '../po',
      install: true,
  install_dir: datadir / 'applications'
)

desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('Validate desktop file', desktop_utils, args: [desktop_file])
endif

appstream_file = i18n.merge_file(
        input: 'io.github.noobping.PasswordStore.metainfo.xml.in',
       output: 'io.github.noobping.PasswordStore.metainfo.xml',
       po_dir: '../po',
      install: true,
  install_dir: datadir / 'metainfo'
)

appstreamcli = find_program('appstreamcli', required: false, disabler: true)
test('Validate appstream file', appstreamcli,
     args: ['validate', '--no-net', '--explain', appstream_file])

install_data('io.github.noobping.PasswordStore.gschema.xml',
  install_dir: datadir / 'glib-2.0' / 'schemas'
)

compile_schemas = find_program('glib-compile-schemas', required: false, disabler: true)
test('Validate schema file',
     compile_schemas,
     args: ['--strict', '--dry-run', meson.current_source_dir()])

service_conf = configuration_data()
service_conf.set('bindir', bindir)
service_conf.set('datadir', datadir)

search_provider = configure_file(
	input: 'io.github.noobping.PasswordStore.SearchProvider.ini.in',
       output: 'io.github.noobping.PasswordStore.SearchProvider.ini',
configuration: service_conf,
      install: true,
  install_dir: datadir / 'gnome-shell' / 'search-providers'
)

search_service = configure_file(
        input: 'io.github.noobping.PasswordStore.SearchProvider.service.in',
       output: 'io.github.noobping.PasswordStore.SearchProvider.service',
configuration: service_conf,
      install: true,
  install_dir: datadir / 'dbus-1' / 'services'
)

subdir('icons')
