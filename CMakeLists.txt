cmake_minimum_required(VERSION 3.24)

project(keycord LANGUAGES NONE)

set(CARGO_VARIANT "developer" CACHE STRING "Cargo-backed wrapper profile")
set_property(CACHE CARGO_VARIANT PROPERTY STRINGS developer release translation)

set(KEYCORD_BINARY "keycord${CMAKE_EXECUTABLE_SUFFIX}")
set(KEYCORD_OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/${KEYCORD_BINARY}")

function(add_cargo_wrapper_target target_name variant output_path all_target)
  add_custom_command(
    OUTPUT "${output_path}"
    COMMAND
      "${CMAKE_COMMAND}"
      "-DSOURCE_DIR=${CMAKE_SOURCE_DIR}"
      "-DBINARY_DIR=${CMAKE_BINARY_DIR}"
      "-DCARGO_VARIANT=${variant}"
      "-DOUTPUT=${output_path}"
      "-DPROJECT_BINARY=${KEYCORD_BINARY}"
      -P
      "${CMAKE_SOURCE_DIR}/cmake/CargoBuild.cmake"
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    USES_TERMINAL
    VERBATIM
  )

  if(all_target)
    add_custom_target("${target_name}" ALL DEPENDS "${output_path}")
  else()
    add_custom_target("${target_name}" DEPENDS "${output_path}")
  endif()
endfunction()

message(STATUS "Configured CMake cargo wrapper variant \"${CARGO_VARIANT}\"")
add_cargo_wrapper_target(keycord "${CARGO_VARIANT}" "${KEYCORD_OUTPUT}" TRUE)

foreach(variant IN ITEMS developer release translation)
  add_cargo_wrapper_target(
    "${variant}-build"
    "${variant}"
    "${CMAKE_CURRENT_BINARY_DIR}/artifacts/${variant}/${KEYCORD_BINARY}"
    FALSE
  )
endforeach()
