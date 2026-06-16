@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0ensure-zig.ps1" %*
if errorlevel 1 exit /b %errorlevel%
