@echo off
rem Type-check shim for cross-target `cargo check` on Windows hosts.
rem `cargo check` never links; ring's build script only needs a tool that
rem reports a compiler family and creates its scratch outputs. The real C
rem compilation is still covered by the Linux job in CI.
setlocal enabledelayedexpansion
set OUT=
set MODE=
:loop
if "%~1"=="" goto done
if /i "%~1"=="-o" (
  set OUT=%~2
  shift
  shift
  goto loop
)
if /i "%~1"=="--version" set MODE=version
if "%~1"=="-v" set MODE=verbose
shift
goto loop
:done
if "%MODE%"=="version" (
  echo gcc ^(GCC^) 12.2.0
  exit /b 0
)
if "%MODE%"=="verbose" (
  echo gcc version 12.2.0
  exit /b 0
)
if defined OUT type nul > "%OUT%"
exit /b 0
