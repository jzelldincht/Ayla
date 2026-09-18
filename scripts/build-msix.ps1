# Alya MSIX 打包脚本（MS Store 准备；轮 17）。
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-msix.ps1 [-Install] [-Sign]
# 默认：从 package.json 读版本（补第 4 位），输出 build\msix\Alya_<ver>_x64.msix（未签名）。
# -Sign：生成/复用自签名 CodeSigning 证书（Subject=Publisher）并签名（本地分发测试用）。
# -Install：Add-AppxPackage 安装到本机（未签名时走 -AllowUnsigned，需 Win11 22000+）。
# 上架：Partner Center 分配 Publisher 后用 -Publisher "CN=<分配值>" 重打包，无需 -AllowUnsigned。
param(
    [string]$Publisher = "CN=AylaTest",
    [string]$PublisherName = "Jzelldincht",
    [string]$IdentityName,
    [string]$Version,
    [switch]$Sign,
    [switch]$Install
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# 包身份：签名模式用正式 Publisher；未签名本地测试（-AllowUnsigned）按微软规则
# Publisher 必须带保留 OID 后缀（learn.microsoft.com/windows/msix/package/unsigned-package），
# 且含可执行内容的安装需管理员 PowerShell。
$unsignedOid = 'OID.2.25.311729368913984317654407730594956997722=1'
if (-not $IdentityName) { $IdentityName = 'Jzelldincht.Ayla' }
if (-not $Sign) { $Publisher = "$Publisher, $unsignedOid" }

# 版本：package.json x.y.z → x.y.z.0（MSIX 需四段）
if (-not $Version) {
    $pkg = Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json
    $Version = "$($pkg.version).0"
}

# SDK 工具解析：bin\<version>\x64\，按版本降序取第一个存在者
$sdkRoot = 'C:\Program Files (x86)\Windows Kits\10\bin'
$sdkDirs = Get-ChildItem $sdkRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $null -ne ($_.Name -as [version]) } |
    Sort-Object { [version]$_.Name } -Descending
$makeappx = $null
$signtool = $null
foreach ($dir in $sdkDirs) {
    if (-not $makeappx) {
        $candidate = Join-Path $dir.FullName 'x64\MakeAppx.exe'
        if (Test-Path $candidate) { $makeappx = $candidate }
    }
    if (-not $signtool) {
        $candidate = Join-Path $dir.FullName 'x64\signtool.exe'
        if (Test-Path $candidate) { $signtool = $candidate }
    }
    if ($makeappx -and $signtool) { break }
}
if (-not $makeappx) { throw "MakeAppx.exe not found (Windows SDK required)" }
if (-not $signtool) { throw "signtool.exe not found (Windows SDK required)" }

# ---- 布局 ----
$layout = Join-Path $root 'build\msix\layout'
$outDir = Join-Path $root 'build\msix'
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $layout | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $layout 'Assets') | Out-Null

$exe = Join-Path $root 'src-tauri\target\release\alya.exe'
if (-not (Test-Path $exe)) { throw "release exe not found; run 'pnpm tauri build' first" }
Copy-Item $exe (Join-Path $layout 'Alya.exe') -Force

$icons = Join-Path $root 'src-tauri\icons'
$assetsDir = Join-Path $layout 'Assets'
foreach ($n in @('Square44x44Logo.png','Square71x71Logo.png','Square150x150Logo.png','Square310x310Logo.png','StoreLogo.png')) {
    Copy-Item (Join-Path $icons $n) (Join-Path $assetsDir $n) -Force
}

# ---- 清单渲染 ----
$langs = @('zh-CN','zh-TW','en','ja','ko','fr','de','es','ru')
$resourceLines = ($langs | ForEach-Object { "    <Resource Language=""$_""/>" }) -join "`r`n"
$manifest = Get-Content (Join-Path $root 'scripts\msix\AppxManifest.template.xml') -Raw -Encoding UTF8
$manifest = $manifest.Replace('__IDENTITY__', $IdentityName)
$manifest = $manifest.Replace('__PUBLISHER__', $Publisher)
$manifest = $manifest.Replace('__VERSION__', $Version)
$manifest = $manifest.Replace('__PUBLISHERDISPLAY__', $PublisherName)
$manifest = $manifest.Replace('__RESOURCES__', $resourceLines)
$manifestPath = Join-Path $layout 'AppxManifest.xml'
Set-Content -Path $manifestPath -Value $manifest -Encoding UTF8

# ---- 打包 ----
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$msix = Join-Path $outDir ("Alya_{0}_x64.msix" -f $Version)
& $makeappx pack /o /d $layout /p $msix
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed (exit $LASTEXITCODE)" }

# ---- 签名（可选；本地分发测试）----
if ($Sign) {
    $cert = Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq $Publisher -and $_.HasPrivateKey } |
        Sort-Object NotAfter -Descending | Select-Object -First 1
    if (-not $cert) {
        $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Publisher `
            -KeyUsage DigitalSignature -KeyExportPolicy Exportable `
            -FriendlyName 'Alya MSIX test signing' -CertStoreLocation Cert:\CurrentUser\My
        Write-Output ("created self-signed cert: " + $cert.Thumbprint)
    }
    $pfxPath = Join-Path $outDir 'ayla-test-signing.pfx'
    $pwdSecure = ConvertTo-SecureString -String 'AlyaTestPfx123!' -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pwdSecure | Out-Null
    & $signtool sign /fd SHA256 /a /f $pfxPath /p 'AlyaTestPfx123!' $msix
    if ($LASTEXITCODE -ne 0) { throw "signtool failed" }
    try {
        Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\LocalMachine\TrustedPeople -Password $pwdSecure | Out-Null
        Write-Output 'cert imported to LocalMachine\TrustedPeople'
    } catch {
        Write-Warning 'cert import to Trusted People failed (admin required); unsigned install uses -AllowUnsigned path'
    }
}

# ---- 安装（可选）----
if ($Install) {
    if ($Sign) {
        Add-AppxPackage -Path $msix
    } else {
        # 未签名安装：Win11 22000+ 支持 -AllowUnsigned
        Add-AppxPackage -Path $msix -AllowUnsigned
    }
    $pkg = Get-AppxPackage $IdentityName
    $pfn = $pkg.PackageFamilyName
    Write-Output ("installed. PackageFamilyName=" + $pfn)
    Write-Output ("launch: explorer.exe shell:AppsFolder\" + $pfn + "!Ayla")
}

Write-Output ("MSIX generated: " + $msix)
