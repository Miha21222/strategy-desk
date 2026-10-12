@echo off
rem Builds the Strategy Desk installer for the two traders of the macro-review
rem MVP: tvmcp + uv + the vault template, WITHOUT the course knowledge base
rem (SD_BUNDLE_KB=0 — the course is copyrighted and this build goes to other
rem people). Run from the repo root on Windows, with node_modules installed.
rem
rem Steps (same as `npm run dist`): stage tvmcp, stage an EMPTY gt-rag payload,
rem stage the vault template out of the owner's vault, build, pack with NSIS.
setlocal
cd /d "%~dp0"
set SD_BUNDLE_KB=0
echo [build-installer] knowledge base: OFF (SD_BUNDLE_KB=0)
call npm run dist
if errorlevel 1 (
  echo [build-installer] FAILED
  exit /b 1
)
echo.
echo [build-installer] done. The installer:
for %%f in ("release\Strategy-Desk-*.exe") do echo   %%~ff
echo.
echo Next: npm run dist:check -- --rename-project   (launches release\win-unpacked headlessly)
endlocal
