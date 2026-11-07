@echo off
rem Runs a script in Git Bash with the MSVC environment (cl, link, the SDK):
rem   scripts\msvc.bat scripts/build-windows.sh llvm
for /f "usebackq delims=" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "vs=%%i"
if not defined vs (echo no Visual Studio with the C++ tools 1>&2 & exit /b 2)
set "PATH=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer;%PATH%"
call "%vs%\VC\Auxiliary\Build\vcvars64.bat" > nul
"%ProgramFiles%\Git\bin\bash.exe" %*
