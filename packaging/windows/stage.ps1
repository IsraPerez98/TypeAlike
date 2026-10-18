<#
.SYNOPSIS
Copies the files of a Windows installer from a Cargo release folder to one folder.

.DESCRIPTION
Copies GhostType.exe, the llama.cpp DLLs and the Windows App Runtime of a build with the
self-contained feature. It also copies the Visual C++ runtime DLLs from Visual Studio, because a
new Windows installation does not always have them. With -CudaPath, it also copies the cuBLAS
DLLs that ggml-cuda.dll loads.

.EXAMPLE
packaging\windows\stage.ps1 -Release C:\t\release -Destination dist\app -CudaPath C:\cuda
#>
param(
    [Parameter(Mandatory)] [string] $Release,
    [Parameter(Mandatory)] [string] $Destination,
    [string] $CudaPath
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$cargoFolders = 'build', 'deps', 'examples', 'incremental', '.fingerprint'
$cargoFiles = '*.d', '*.pdb', '*.lock', '.cargo-*'
$visualCppRuntime = 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'

function Test-CargoOutput([System.IO.FileSystemInfo] $Item) {
    if ($Item.PSIsContainer) {
        return $Item.Name -in $cargoFolders
    }
    return [bool]($cargoFiles | Where-Object { $Item.Name -like $_ })
}

function Get-VisualCppRuntimeFolder {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $visualStudio = & $vswhere -latest -products * -property installationPath
    $runtime = Get-ChildItem "$visualStudio\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT\vcruntime140.dll" |
        Sort-Object { $_.VersionInfo.FileVersionRaw } |
        Select-Object -Last 1
    if (-not $runtime) {
        throw "no Visual C++ runtime in $visualStudio"
    }
    return $runtime.DirectoryName
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null

Get-ChildItem -LiteralPath $Release -Force |
    Where-Object { -not (Test-CargoOutput $_) } |
    Copy-Item -Destination $Destination -Recurse -Force

$runtimeFolder = Get-VisualCppRuntimeFolder
$visualCppRuntime | ForEach-Object { Copy-Item (Join-Path $runtimeFolder $_) $Destination -Force }

$required = 'GhostType.exe', 'Microsoft.WindowsAppRuntime.dll', 'ggml-vulkan.dll'
if ($CudaPath) {
    Get-ChildItem -LiteralPath $CudaPath -Recurse -Include 'cublas64_*.dll', 'cublasLt64_*.dll' |
        Copy-Item -Destination $Destination -Force
    $required += 'ggml-cuda.dll', 'cublas64_*.dll', 'cublasLt64_*.dll'
}

$missing = $required | Where-Object { -not (Test-Path (Join-Path $Destination $_)) }
if ($missing) {
    throw "the installer folder has no $($missing -join ', ')"
}
