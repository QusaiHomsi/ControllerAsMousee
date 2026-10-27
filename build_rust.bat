@echo off
setlocal
cd /d "%~dp0"

set "CARGO=cargo"
where cargo >nul 2>nul
if errorlevel 1 if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if "%CARGO%"=="cargo" where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust and Cargo were not found. Install the stable Rust toolchain, then reopen this terminal.
  pause
  exit /b 1
)

echo Building Controller Control in release mode...
"%CARGO%" build --release
if errorlevel 1 (
  echo.
  echo BUILD FAILED - see the Rust compiler output above.
  pause
  exit /b 1
)

if not exist "target\release\controller_to_pc.exe" (
  echo.
  echo Build finished but target\release\controller_to_pc.exe is missing.
  pause
  exit /b 1
)

for /f "usebackq delims=" %%D in (`powershell -NoProfile -Command "[Environment]::GetFolderPath('Desktop')"`) do set "DESKTOP=%%D"
copy /Y "target\release\controller_to_pc.exe" "%DESKTOP%\controller_to_pc.exe"
if errorlevel 1 (
  echo.
  echo Could not replace the Desktop copy. Close Controller Control and run this file again.
  pause
  exit /b 1
)

echo.
echo SUCCESS. Rust build copied to your Desktop: %DESKTOP%
pause
