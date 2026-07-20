@echo off
setlocal
set "SACCADE_CURRENT_AGENT_POINTER=%LOCALAPPDATA%\Saccade\CEF\Agent\current-grant-path"
set "SACCADE_APP_EXECUTABLE=%~dp0Saccade.exe"
"%~dp0saccade-mcp.exe" serve-stdio
