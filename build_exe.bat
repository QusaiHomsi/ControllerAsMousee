@echo off
setlocal
cd /d "%~dp0"

REM Prefer Python versions with ready-made pygame and PyInstaller packages.
set "PY="
for %%V in (3.12 3.13 3.11) do (
  if not defined PY (
    py -%%V --version >nul 2>nul && set "PY=py -%%V"
  )
)
if not defined PY (
  echo.
  echo Python 3.12 was not found. Install Python 3.12 and run this file again.
  echo https://www.python.org/downloads/release/python-3129/
  pause
  exit /b 1
)

echo Using: %PY%
%PY% --version
echo.
echo Installing Python packages...
%PY% -m pip install --upgrade pip --no-warn-script-location
%PY% -m pip install --only-binary=pygame,pillow --no-warn-script-location pygame pillow pystray pyinstaller
if errorlevel 1 (
  echo.
  echo PACKAGE INSTALL FAILED - see the messages above.
  pause
  exit /b 1
)

echo.
echo Building Python application...
%PY% -m PyInstaller --onefile --noconsole --name controller_to_pc --hidden-import pystray._win32 controller_to_pc.py
if errorlevel 1 (
  echo.
  echo BUILD FAILED - see the messages above.
  pause
  exit /b 1
)

if not exist "dist\controller_to_pc.exe" (
  echo.
  echo Build finished but dist\controller_to_pc.exe is missing.
  pause
  exit /b 1
)

for /f "usebackq delims=" %%D in (`powershell -NoProfile -Command "[Environment]::GetFolderPath('Desktop')"`) do set "DESKTOP=%%D"
copy /Y "dist\controller_to_pc.exe" "%DESKTOP%\controller_to_pc.exe"
if errorlevel 1 (
  echo.
  echo Could not replace the Desktop copy. Close Controller Control and run this file again.
  pause
  exit /b 1
)

echo.
echo SUCCESS. Python build copied to your Desktop: %DESKTOP%
pause
