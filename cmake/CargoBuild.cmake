if(NOT DEFINED SOURCE_DIR OR NOT DEFINED BINARY_DIR OR NOT DEFINED CARGO_VARIANT OR NOT DEFINED OUTPUT OR NOT DEFINED PROJECT_BINARY)
  message(FATAL_ERROR "Missing required CMake cargo wrapper variables.")
endif()

if(CARGO_VARIANT STREQUAL "developer")
  set(cargo_profile "debug")
  set(cargo_args build)
  set(cargo_summary "cargo build")
elseif(CARGO_VARIANT STREQUAL "release" OR CARGO_VARIANT STREQUAL "translation")
  set(cargo_profile "release")
  set(cargo_args build --release)
  set(cargo_summary "cargo build --release")
else()
  message(FATAL_ERROR "Unsupported cargo wrapper profile: ${CARGO_VARIANT}")
endif()

function(resolve_cargo out_var)
  if(DEFINED CARGO_EXECUTABLE AND NOT CARGO_EXECUTABLE STREQUAL "")
    set(${out_var} "${CARGO_EXECUTABLE}" PARENT_SCOPE)
    return()
  endif()

  find_program(local_cargo cargo)
  if(local_cargo)
    set(${out_var} "${local_cargo}" PARENT_SCOPE)
    return()
  endif()

  find_program(flatpak_spawn flatpak-spawn)
  if(flatpak_spawn)
    execute_process(
      COMMAND "${flatpak_spawn}" --host which cargo
      OUTPUT_VARIABLE host_cargo
      OUTPUT_STRIP_TRAILING_WHITESPACE
      RESULT_VARIABLE host_cargo_result
      ERROR_QUIET
    )
    if(host_cargo_result EQUAL 0 AND NOT host_cargo STREQUAL "")
      set(${out_var} "${flatpak_spawn};--host;${host_cargo}" PARENT_SCOPE)
      return()
    endif()

    execute_process(
      COMMAND "${flatpak_spawn}" --host which toolbox
      RESULT_VARIABLE toolbox_result
      OUTPUT_QUIET
      ERROR_QUIET
    )
    if(toolbox_result EQUAL 0)
      execute_process(
        COMMAND "${flatpak_spawn}" --host toolbox run which cargo
        RESULT_VARIABLE toolbox_cargo_result
        OUTPUT_QUIET
        ERROR_QUIET
      )
      if(toolbox_cargo_result EQUAL 0)
        set(${out_var} "${flatpak_spawn};--host;toolbox;run;cargo" PARENT_SCOPE)
        return()
      endif()
    endif()
  endif()

  message(FATAL_ERROR "Unable to find cargo in PATH, through flatpak-spawn --host, or through toolbox.")
endfunction()

resolve_cargo(cargo_command)
set(ENV{CARGO_TARGET_DIR} "${BINARY_DIR}/cargo-target")
file(MAKE_DIRECTORY "$ENV{CARGO_TARGET_DIR}")

get_filename_component(output_dir "${OUTPUT}" DIRECTORY)
file(MAKE_DIRECTORY "${output_dir}")

message(STATUS "CMake Cargo wrapper: ${CARGO_VARIANT} -> ${cargo_summary}")
execute_process(
  COMMAND ${cargo_command} ${cargo_args}
  WORKING_DIRECTORY "${SOURCE_DIR}"
  COMMAND_ECHO STDOUT
  RESULT_VARIABLE cargo_result
)

if(NOT cargo_result EQUAL 0)
  message(FATAL_ERROR "Cargo build failed with exit code ${cargo_result}.")
endif()

set(artifact_path "$ENV{CARGO_TARGET_DIR}/${cargo_profile}/${PROJECT_BINARY}")
if(NOT EXISTS "${artifact_path}")
  message(FATAL_ERROR "Expected Cargo artifact not found: ${artifact_path}")
endif()

file(COPY_FILE "${artifact_path}" "${OUTPUT}" ONLY_IF_DIFFERENT)
