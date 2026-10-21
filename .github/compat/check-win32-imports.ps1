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

$exes = Get-ChildItem "$Target\debug\deps\aurea-*.exe", "$Target\debug\examples\*.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -ge ($libs | Measure-Object LastWriteTime -Maximum).Maximum }
$v5 = $exes | Where-Object { (& $dumpbin /nologo /dependents $_.FullName) -match '^\s+comctl32\.dll$' }
if ($v5) {
    Write-Host "Linked to comctl32 v5, which loads before Aurea switches to v6:"
    $v5 | ForEach-Object { Write-Host "  $($_.Name)" }
    exit 1
}
Write-Host "$(@($exes).Count) executables, none linked to comctl32"
