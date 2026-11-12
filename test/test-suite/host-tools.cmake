# Included into llvm-test-suite's project (CMAKE_PROJECT_INCLUDE) by
# scripts/run-test-suite.sh.
# - With TEST_SUITE_USER_MODE_EMULATION the tests run timeit and fpcmp on
#   the host, but the test-suite still compiles timeit-target (timeit.c
#   needs <sys/resource.h>) and not (Fortran only) for the target, which
#   newlib cannot: they become imported executables, which nothing
#   compiles.
# - The start-up code, system calls and linker script (RX_TS_LINK_DEPENDS)
#   are only in the linker flags: they become link dependencies of every
#   program, so that a change to them relinks it.
function(add_executable name)
  if(name STREQUAL "timeit-target" OR name STREQUAL "not")
    _add_executable(${name} IMPORTED GLOBAL)
    set_target_properties(${name} PROPERTIES
      IMPORTED_LOCATION ${CMAKE_BINARY_DIR}/tools/${name}-unused)
    return()
  endif()
  _add_executable(${ARGV})
  if(NOT "ALIAS" IN_LIST ARGV AND NOT "IMPORTED" IN_LIST ARGV)
    set_property(TARGET ${name} APPEND PROPERTY LINK_DEPENDS ${RX_TS_LINK_DEPENDS})
  endif()
endfunction()
