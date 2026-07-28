@echo off
setlocal
cd /d "%~dp0"

where pwsh.exe >nul 2>nul
if errorlevel 1 (
    echo PowerShell 7 is required. Install Microsoft.PowerShell with winget first.
    pause
    exit /b 1
)

pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Update-Hermes-Agent.ps1" -Mode Apply
set "EXIT_CODE=%ERRORLEVEL%"

echo.
if not "%EXIT_CODE%"=="0" (
    echo Hermes Agent update failed. Review logs\update\update.log.
) else (
    echo Hermes Agent update completed successfully.
)
pause
exit /b %EXIT_CODE%
