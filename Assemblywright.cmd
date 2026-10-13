@echo off
setlocal
set "ASSEMBLYWRIGHT_GUI_EXE=%~dp0target\windows-gui\Assemblywright.exe"
if exist "%ASSEMBLYWRIGHT_GUI_EXE%" goto launch
powershell.exe -NoProfile -File "%~dp0scripts\build-windows-gui.ps1"
if errorlevel 1 (
  echo Assemblywright could not be built. See the error above.
  pause
  exit /b 1
)
:launch
start "" "%ASSEMBLYWRIGHT_GUI_EXE%"
