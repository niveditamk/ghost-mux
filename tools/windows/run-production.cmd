@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-production.ps1" %*
if errorlevel 1 exit /b %errorlevel%
