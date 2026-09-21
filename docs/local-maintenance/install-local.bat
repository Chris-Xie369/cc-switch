@echo off
REM ---------------------------------------------------------------------------
REM Install the locally built CC Switch into the EXISTING install directory.
REM
REM Why a .bat: Git Bash rewrites /S and /D= arguments (MSYS path conversion),
REM so invoking the NSIS installer directly from bash silently mis-installs to
REM the NSIS default directory instead. This file keeps the arguments intact.
REM
REM Picks the newest CC Switch_*_x64-setup.exe from the bundle dir, so it does
REM not need editing when the version number changes.
REM /D= must be the LAST parameter and must NOT be quoted.
REM ---------------------------------------------------------------------------

set NSISDIR=D:\Workspace\Project\cc-switch\src\src-tauri\target\release\bundle\nsis
set TARGET=C:\Users\Jason\AppData\Local\Programs\CC Switch
set SETUP=

if not exist "%NSISDIR%" (
  echo FATAL: bundle dir not found: %NSISDIR%
  exit /b 1
)

for /f "delims=" %%i in ('dir /b /o-d "%NSISDIR%\CC Switch_*_x64-setup.exe" 2^>nul') do (
  if not defined SETUP set "SETUP=%NSISDIR%\%%i"
)

if not defined SETUP (
  echo FATAL: no installer found in %NSISDIR%
  exit /b 1
)

echo SETUP=%SETUP%
echo TARGET=%TARGET%

"%SETUP%" /S /D=%TARGET%
echo EXITCODE=%ERRORLEVEL%
