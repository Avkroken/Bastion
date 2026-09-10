@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%Android\gradlew.bat" --project-dir "%SCRIPT_DIR%Android" %*
exit /b %ERRORLEVEL%
