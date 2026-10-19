# Release installer for Windows x64: a per-user NSIS setup, the updater's target.
#
#   MIO_PRODUCT_NAME   product name       (default: Mio Music)
#   MIO_BUNDLE_ID      bundle identifier  (default: com.vant.Mio.Music)
#   MIO_PUBLISHER      registry vendor    (default: vant, as Tauri derives it from the identifier)
#   MIO_OUT            output directory   (default: dist)
#   MIO_DAV1D_BIN      folder holding dav1d.dll (default: the vcpkg x64-windows bin folder)
#
# The asset name follows tauri-action's: dots instead of spaces, `_<version>_x64-setup.exe`.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$product = if ($env:MIO_PRODUCT_NAME) { $env:MIO_PRODUCT_NAME } else { 'Mio Music' }
$identifier = if ($env:MIO_BUNDLE_ID) { $env:MIO_BUNDLE_ID } else { 'com.vant.Mio.Music' }
$publisher = if ($env:MIO_PUBLISHER) { $env:MIO_PUBLISHER } else { 'vant' }
$out = if ($env:MIO_OUT) { $env:MIO_OUT } else { 'dist' }
$version = if ($env:MIO_VERSION) { $env:MIO_VERSION } else { (python -I tools/ci/version.py).Trim() }
$stem = $product -replace ' ', '.'
$dav1dBin = if ($env:MIO_DAV1D_BIN) { $env:MIO_DAV1D_BIN } else { Join-Path $env:VCPKG_INSTALLATION_ROOT 'installed\x64-windows\bin' }

cargo build --release --locked -p mio-app
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$out = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $out))
$stage = Join-Path $out 'windows-stage'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item 'target\release\mio-app.exe' $stage
Copy-Item (Join-Path $dav1dBin 'dav1d.dll') $stage
Copy-Item 'crates\mio-app\assets\icons\TDesign-LICENSE.txt' $stage
Copy-Item 'crates\mio-app\src\shell\background-LICENSES.txt' (Join-Path $stage 'BackgroundFilterLicenses.txt')

$installer = Join-Path $out "${stem}_${version}_x64-setup.exe"
$makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $makensis) { $makensis = 'C:\Program Files (x86)\NSIS\makensis.exe' }
& $makensis /V2 `
    "/DPRODUCT_NAME=$product" "/DVERSION=$version" "/DBUNDLE_ID=$identifier" "/DPUBLISHER=$publisher" `
    "/DSTAGE_DIR=$stage" "/DOUT_FILE=$installer" "/DICON_FILE=$(Resolve-Path 'packaging\icons\icon.ico')" `
    'packaging\windows\installer.nsi'
if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
Remove-Item -Recurse -Force $stage
Write-Output $installer
