<#
.SYNOPSIS
Copies the files of a Windows installer from a Cargo release folder to one folder.

.DESCRIPTION
Copies TypeAlike.exe, the llama.cpp DLLs and the Windows App Runtime of a build with the
self-contained feature, and the license. The backend DLLs of llama.cpp stay in their folders in
backends (cpu, vulkan, cuda), so that TypeAlike loads only the backends that it uses. It also
copies the Visual C++ runtime DLLs from Visual Studio, because a new Windows installation does not
always have them. With -CudaPath, it also copies the CUDA runtime and cuBLAS DLLs that
ggml-cuda.dll loads. Then it makes sure that Windows or the folder has each DLL that TypeAlike.exe
and the llama.cpp DLLs load.

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
$cudaDlls = 'cudart64_*.dll', 'cublas64_*.dll', 'cublasLt64_*.dll'
$checkedBinaries = @('TypeAlike.exe', 'llama*.dll', 'ggml*.dll') + $cudaDlls
$cpuBackend = 'backends\cpu\ggml-cpu-*.dll'
$vulkanBackend = 'backends\vulkan\ggml-vulkan.dll'
$cudaBackend = 'backends\cuda\ggml-cuda.dll'
# The GPU driver installs these, and llama.cpp skips the backend DLL when the driver is not there.
$driverDlls = 'nvcuda.dll', 'vulkan-1.dll'
# Windows also installs propsys.dll, but its product name is Windows Search.
$windowsProducts = '*Windows*Operating System*', 'Windows*Search'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$visualStudio = & $vswhere -latest -products * -property installationPath

function Test-CargoOutput([System.IO.FileSystemInfo] $Item) {
    if ($Item.PSIsContainer) {
        return $Item.Name -in $cargoFolders
    }
    return [bool]($cargoFiles | Where-Object { $Item.Name -like $_ })
}

function Get-VisualCppRuntimeFolder {
    $runtime = Get-ChildItem "$visualStudio\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT\vcruntime140.dll" |
        Sort-Object { $_.VersionInfo.FileVersionRaw } |
        Select-Object -Last 1
    if (-not $runtime) {
        throw "no Visual C++ runtime in $visualStudio"
    }
    return $runtime.DirectoryName
}

function Test-WindowsDll([string] $Name) {
    if ($Name -like 'api-ms-win-*' -or $Name -like 'ext-ms-*') {
        return $true
    }
    $dll = Get-Item (Join-Path "$env:SystemRoot\System32" $Name) -ErrorAction SilentlyContinue
    if (-not $dll) {
        return $false
    }
    return [bool]($windowsProducts | Where-Object { $dll.VersionInfo.ProductName -like $_ })
}

# Windows looks for the DLLs that a backend DLL loads in the folder of TypeAlike.exe, not in the
# folder of the backend DLL. Thus each of these DLLs must be in the top folder.
function Get-UnshippedDlls([System.IO.FileInfo] $Binary, [string] $Dumpbin) {
    $name = [System.IO.Path]::GetRelativePath($Destination, $Binary.FullName)
    & $Dumpbin /nologo /dependents $Binary.FullName |
        ForEach-Object { if ($_ -match '^\s+(\S+\.dll)\s*$') { $Matches[1] } } |
        Where-Object {
            $_ -notin $driverDlls -and
            -not (Test-Path -LiteralPath (Join-Path $Destination $_)) -and
            -not (Test-WindowsDll $_)
        } |
        ForEach-Object { "$name loads $_" }
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$Destination = Convert-Path -LiteralPath $Destination

Get-ChildItem -LiteralPath $Release -Force |
    Where-Object { -not (Test-CargoOutput $_) } |
    Copy-Item -Destination $Destination -Recurse -Force

$runtimeFolder = Get-VisualCppRuntimeFolder
$visualCppRuntime | ForEach-Object { Copy-Item (Join-Path $runtimeFolder $_) $Destination -Force }
Copy-Item (Join-Path $PSScriptRoot '..\..\LICENSE') $Destination -Force

$required = 'TypeAlike.exe', 'Microsoft.WindowsAppRuntime.dll', $cpuBackend, $vulkanBackend, 'LICENSE'
if ($CudaPath) {
    Get-ChildItem -LiteralPath $CudaPath -Recurse -Include $cudaDlls |
        Copy-Item -Destination $Destination -Force
    $required += @($cudaBackend) + $cudaDlls
}

$missing = $required | Where-Object { -not (Test-Path (Join-Path $Destination $_)) }
if ($missing) {
    throw "the installer folder has no $($missing -join ', ')"
}

$dumpbin = Get-ChildItem "$visualStudio\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Select-Object -Last 1
if (-not $dumpbin) {
    throw "no dumpbin.exe in $visualStudio"
}
$unshipped = Get-ChildItem -LiteralPath $Destination -Recurse -Include $checkedBinaries |
    ForEach-Object { Get-UnshippedDlls $_ $dumpbin.FullName }
if ($unshipped) {
    throw "the installer folder and Windows do not have these DLLs: $($unshipped -join ', ')"
}
