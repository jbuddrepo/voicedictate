# The Rust cmake wrapper supplies explicit MSVC release flags and can replace
# CMake's optimized defaults. Keep the inference core optimized even then.
# These are optimization settings only; CPU instruction gates remain in the
# baseline/AVX2 hooks. Leave Debug configurations unchanged.
if(MSVC)
  foreach(language C CXX)
    foreach(configuration RELEASE RELWITHDEBINFO)
      set(flag_name "CMAKE_${language}_FLAGS_${configuration}")
      set(release_flags "${${flag_name}}")
      # Remove lower/no optimization options before appending speed optimization.
      # Iterate because adjacent options may share the whitespace delimiter.
      while(release_flags MATCHES "(^|[ \t])[-/][Oo]([dD]|[01])([ \t]|$)")
        string(REGEX REPLACE "(^|[ \t])[-/][Oo]([dD]|[01])([ \t]|$)" " " release_flags "${release_flags}")
      endwhile()
      if(NOT release_flags MATCHES "(^|[ \t])[-/][Oo]2([ \t]|$)")
        string(APPEND release_flags " /O2")
      endif()
      if(NOT release_flags MATCHES "(^|[ \t])[-/][Dd]NDEBUG([ \t]|$)")
        string(APPEND release_flags " /DNDEBUG")
      endif()
      string(STRIP "${release_flags}" release_flags)
      set(${flag_name} "${release_flags}" CACHE STRING "MSVC release inference flags" FORCE)
    endforeach()
  endforeach()
endif()
