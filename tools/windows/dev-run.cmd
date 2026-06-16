@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0dev-run.ps1" %*
if errorlevel 1 exit /b %errorlevel%
