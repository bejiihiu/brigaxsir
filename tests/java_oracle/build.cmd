@echo off
rem Regenerates the differential-oracle classes for tests/java_oracle (task 20).
rem The cargo test does this automatically into target\java-oracle\classes;
rem this script exists for manual runs of the harness:
rem   java -cp target\java-oracle\classes OracleHarness < scenarios.txt
setlocal
set CLASSES=%~dp0..\..\target\java-oracle\classes
set LIB=..\brigadier\src\main\java

if not exist "%LIB%\com\mojang\brigadier\CommandDispatcher.java" (
    echo Java library sources not found at %LIB% >&2
    exit /b 1
)
where javac >nul 2>nul
if errorlevel 1 (
    echo javac not found on PATH >&2
    exit /b 1
)

if not exist "%CLASSES%" mkdir "%CLASSES%"
rem collect every library .java plus the harness and compile in one go
setlocal enabledelayedexpansion
set FILES=
for /r "%LIB%" %%f in (*.java) do set FILES=!FILES! "%%f"
javac -encoding UTF-8 -d "%CLASSES%" !FILES! "%~dp0OracleHarness.java"
if errorlevel 1 (
    echo javac failed >&2
    exit /b 1
)
echo compiled into %CLASSES%
endlocal
