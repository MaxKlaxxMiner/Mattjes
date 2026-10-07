@echo off
setlocal enabledelayedexpansion
rem Generates and measures all endgame tables with a given number of pieces
rem (default 5) with the Rust binary: one process per material, so memory is
rem freed in between; dependencies are loaded from the cache directory.
rem
rem Output:
rem   console                             header per material, retrograde levels, summary
rem   mattjes-egtb-cache\console.log      the complete console output (UTF-8), for grepping
rem                                       "panicked", "WARNING", "MISMATCH"
rem   mattjes-egtb-cache\measure.log      one line per table with all values and the checksum
rem
rem   measure-egtb.bat            all five-piece materials, 12 workers, with verification
rem   measure-egtb.bat 5 14       all five-piece materials with 14 workers (home machine)
rem   measure-egtb.bat 6 14 no    six pieces, 14 workers, without the forward verification

set PIECES=%1
if "%PIECES%"=="" set PIECES=5
set WORKERS=%2
if "%WORKERS%"=="" set WORKERS=12
set VERIFY=--verify
if /i "%3"=="no" set VERIFY=
set EXE=runMattjesRs.exe
set CACHE=mattjes-egtb-cache
set LOG=%CACHE%\measure.log
set CONSOLE=%CACHE%\console.log

if not exist %EXE% (
    echo %EXE% not found, run build.sh first
    exit /b 1
)
if not exist %CACHE% mkdir %CACHE%

set N=0
for /f %%m in ('%EXE% egtb-list %PIECES%') do set /a N+=1
echo %DATE% %TIME% start: %N% materials with %PIECES% pieces, %WORKERS% workers %VERIFY% >> %LOG%
echo %DATE% %TIME% start: %N% materials with %PIECES% pieces, %WORKERS% workers %VERIFY% >> %CONSOLE%

set I=0
for /f %%m in ('%EXE% egtb-list %PIECES%') do (
    set /a I+=1
    echo.
    echo ===== !I! / %N% - %%m   %DATE% !TIME! =====
    echo. >> %CONSOLE%
    echo ===== !I! / %N% - %%m   %DATE% !TIME! ===== >> %CONSOLE%
    rem PowerShell as tee: every line goes to the console and to console.log at once,
    rem stderr (panics) included; the exit code of the binary is passed through
    powershell -NoProfile -Command "& '.\%EXE%' egtb-measure %%m --workers %WORKERS% %VERIFY% 2>&1 | ForEach-Object { $_ | Out-Host; $_ } | Out-File -FilePath '%CONSOLE%' -Append -Encoding utf8; exit $LASTEXITCODE"
    if errorlevel 1 (
        echo FAILED %%m >> %LOG%
        echo FAILED %%m >> %CONSOLE%
        echo FAILED %%m
        exit /b 1
    )
)
echo %DATE% %TIME% done: %N% materials with %PIECES% pieces >> %LOG%
echo %DATE% %TIME% done: %N% materials with %PIECES% pieces >> %CONSOLE%
echo.
echo done: %N% materials with %PIECES% pieces, see %LOG% and %CONSOLE%
rem tables written by an older binary are raw; this rewrites them compressed (already compressed files are skipped)
%EXE% egtb-compress --workers %WORKERS%
