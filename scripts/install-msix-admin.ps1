# 管理员提权安装未签名 MSIX（由 build-msix 流程经 UAC 调起；结果写入 result 文件供回读）
param([string]$MsixPath)
$log = Join-Path (Split-Path -Parent $MsixPath) 'install-result.txt'
try {
    Add-AppxPackage -Path $MsixPath -AllowUnsigned -ErrorAction Stop
    $pkg = Get-AppxPackage Jzelldincht.Ayla
    "SUCCESS|Installed|PFN=$($pkg.PackageFamilyName)|InstallLocation=$($pkg.InstallLocation)" | Out-File -FilePath $log -Encoding UTF8
} catch {
    "FAILED|$($_.Exception.Message)" | Out-File -FilePath $log -Encoding UTF8
}
