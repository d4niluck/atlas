@echo off
setlocal
cd /d "%~dp0"
set "out=%TEMP%\atlas-pdf-smoke-%RANDOM%-%RANDOM%.txt"
if exist "%out%" exit /b 1
atlas-pdfium-extract.exe < "two-pages.pdf" > "%out%"
if errorlevel 1 goto fail
findstr /c:"Atlas first page" "%out%" >nul || goto fail
findstr /c:"Atlas second page" "%out%" >nul || goto fail
del "%out%"
echo PDF helper smoke test passed.
exit /b 0
:fail
if exist "%out%" del "%out%"
echo PDF helper smoke test failed.
exit /b 1
