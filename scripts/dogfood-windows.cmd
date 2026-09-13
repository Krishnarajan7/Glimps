@echo off
rem Runs the PowerShell dogfood helper from cmd.exe, where a bare .ps1 path
rem would open Notepad (the .ps1 file association) instead of executing.
rem Prefers PowerShell 7 when installed; falls back to Windows PowerShell 5.1.
where pwsh >nul 2>nul
if %errorlevel%==0 (
  pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0dogfood-windows.ps1" %*
) else (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0dogfood-windows.ps1" %*
)
exit /b %errorlevel%
