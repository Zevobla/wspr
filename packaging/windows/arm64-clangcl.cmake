# ggml ARM64-Windows toolchain, clang-cl variant. clang-cl is the MSVC-compatible
# driver: it accepts cmake-rs's cl.exe-style flags (-nologo -MD -Brepro) AND still
# reports CMAKE_C_COMPILER_ID == "Clang", so ggml's
# `if (MSVC AND NOT CMAKE_C_COMPILER_ID STREQUAL "Clang")` guard passes. It honours
# INCLUDE/LIB from `vcvarsall arm64`, so it finds the ARM64 MSVC headers/libs. The
# stock cmake/arm64-windows-llvm.cmake uses the GNU `clang` driver, which rejects
# -nologo. Point cmake at this via CMAKE_TOOLCHAIN_FILE with CMAKE_GENERATOR=Ninja.
set( CMAKE_SYSTEM_NAME Windows )
set( CMAKE_SYSTEM_PROCESSOR arm64 )

set( target arm64-pc-windows-msvc )

set( CMAKE_C_COMPILER    clang-cl )
set( CMAKE_CXX_COMPILER  clang-cl )

set( CMAKE_C_COMPILER_TARGET   ${target} )
set( CMAKE_CXX_COMPILER_TARGET ${target} )

# clang cc1 flags routed through clang-cl with /clang: so the MSVC-style driver
# forwards them. NOTE: -march=armv8.7-a matches the dev machine; for broad
# Windows-on-ARM shipping compatibility consider lowering to the WoA baseline
# (e.g. armv8.2-a). Verified building/running with armv8.7-a.
set( arch_c_flags "/clang:-march=armv8.7-a /clang:-fvectorize /clang:-ffp-model=fast /clang:-fno-finite-math-only" )
set( warn_c_flags "-Wno-format -Wno-unused-variable -Wno-unused-function -Wno-gnu-zero-variadic-macro-arguments" )

set( CMAKE_C_FLAGS_INIT   "${arch_c_flags} ${warn_c_flags}" )
set( CMAKE_CXX_FLAGS_INIT "${arch_c_flags} ${warn_c_flags}" )
