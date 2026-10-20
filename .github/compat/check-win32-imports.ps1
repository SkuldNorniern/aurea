param([string]$Target = "target")

$ErrorActionPreference = "Stop"
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$dumpbin = & $vswhere -latest -products * -find "VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Select-Object -First 1
if (-not $dumpbin) { throw "dumpbin not found" }

$libs = Get-ChildItem "$Target\debug\build\aurea-ffi-*\out\native_gui*.lib" |
    Sort-Object LastWriteTime -Descending | Group-Object Name | ForEach-Object { $_.Group[0] }
if (-not $libs) { throw "native_gui libs not found under $Target" }

$imports = foreach ($lib in $libs) {
    & $dumpbin /nologo /symbols $lib.FullName |
        Select-String 'UNDEF.*External\s+\|\s+__imp_(\S+)' |
        ForEach-Object { $_.Matches[0].Groups[1].Value }
}
$allowed = Get-Content "$PSScriptRoot\win32-imports.txt" | ForEach-Object { $_.Trim() } | Where-Object { $_ }
$new = $imports | Sort-Object -Unique -CaseSensitive | Where-Object { $allowed -cnotcontains $_ }

if ($new) {
    Write-Host "Win32 imports not in win32-imports.txt:"
    $new | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "$(($imports | Sort-Object -Unique).Count) Win32 imports, all allowed"
