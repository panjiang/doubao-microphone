"""WSL-only local build orchestrator. Uses the already verified WDK cache.
No downloads, installation, trust-store or boot-policy changes.
"""
from pathlib import Path
import subprocess
import shutil

source = Path(__file__).resolve().parent
build = Path('/mnt/c/Users/Administrator/AppData/Local/Temp/doubao-ptt-umdf-research')
shutil.copytree(source, build, dirs_exist_ok=True, ignore=shutil.ignore_patterns('out'))
out = build / 'out'
out.mkdir(exist_ok=True)
native = r'C:\Users\Administrator\AppData\Local\Temp\doubao-ptt-umdf-research'
msvc = r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207'
sdk = r'C:\Program Files (x86)\Windows Kits\10'
wdk = r'C:\Users\Administrator\AppData\Local\Temp\doubao-ptt-vhf-research\out\toolchain\wdk\c'
batch = fr'''@echo off
set "PATH={msvc}\bin\Hostx64\x64;%SystemRoot%\System32;%SystemRoot%"
set "INCLUDE={wdk}\Include\wdf\umdf\2.33;{sdk}\Include\10.0.26100.0\um;{sdk}\Include\10.0.26100.0\shared;{sdk}\Include\10.0.26100.0\ucrt;{wdk}\Include\10.0.26100.0\km;{wdk}\Include\10.0.26100.0\km\wdm;{msvc}\include"
set "LIB={wdk}\Lib\wdf\umdf\x64\2.33;{sdk}\Lib\10.0.26100.0\um\x64;{sdk}\Lib\10.0.26100.0\ucrt\x64;{msvc}\lib\x64"
cd /d "{native}\out"
cl /nologo /c /TC /std:c17 /MT /O2 /W4 /external:env:INCLUDE /external:W0 /DUNICODE /D_UNICODE /DUMDF_VERSION_MAJOR=2 /DUMDF_VERSION_MINOR=33 /DWINVER=0x0A00 /D_WIN32_WINNT=0x0A00 /DNTDDI_VERSION=0x0A00000B "{native}\driver\vhidmini.c" "{native}\driver\util.c" "{native}\shared\ptt_state.c"
if errorlevel 1 exit /b 1
link /nologo /DLL /EXPORT:FxDriverEntryUm /DYNAMICBASE /NXCOMPAT /HIGHENTROPYVA /DEBUG /OUT:DoubaoPTTUm.dll vhidmini.obj util.obj ptt_state.obj WdfDriverStubUm.lib ntdll.lib mincore.lib
if errorlevel 1 exit /b 1
cl /nologo /TC /std:c17 /MT /W4 /WX /Fe:umdf-probe.exe "{native}\tools\umdf-probe.c" /link hid.lib setupapi.lib user32.lib
if errorlevel 1 exit /b 1
copy /y "{native}\driver\DoubaoPTTUm.inf" DoubaoPTTUm.inf >nul
"{wdk}\tools\10.0.26100.0\x64\infverif.exe" /w /v DoubaoPTTUm.inf
if errorlevel 1 exit /b 1
'''
(out / 'build.cmd').write_bytes(batch.replace('\n', '\r\n').encode('ascii'))
result = subprocess.run(['/mnt/c/Windows/System32/cmd.exe', '/d', '/c', native + r'\out\build.cmd'], cwd=build, capture_output=True, text=True)
print(result.stdout + result.stderr)
(out / 'build.log').write_text(result.stdout + result.stderr)
raise SystemExit(result.returncode)
