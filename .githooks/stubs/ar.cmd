@echo off
rem Companion to gcc.cmd: creates the empty archive ring's build script expects.
setlocal enabledelayedexpansion
set OUT=
:loop
if "%~1"=="" goto done
if /i "%~1"=="rcs" (
  set OUT=%~2
  shift
  goto loop
)
shift
goto loop
:done
if defined OUT type nul > "%OUT%"
exit /b 0
