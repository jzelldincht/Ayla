# Alya 托盘自动化：打开溢出面板并定位 Alya 图标（媒体采集用）
# 输出格式：ICON <x> <y>   （图标中心物理坐标）；失败输出 NOTFOUND <原因>
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$ErrorActionPreference = 'Stop'
$AE = [System.Windows.Automation.AutomationElement]
$TS = [System.Windows.Automation.TreeScope]

function Get-Tray ([string]$cls) {
  $root = $AE::RootElement
  $c = New-Object System.Windows.Automation.PropertyCondition($AE::ClassNameProperty, $cls)
  return $root.FindFirst($TS::Children, $c)
}

# 1) 若溢出面板未开，点开它
$overflowBtn = $null
$tray = Get-Tray 'Shell_TrayWnd'
$btns = $tray.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($e in $btns) {
  if ($e.Current.Name -like '显示隐藏的图标*') { $overflowBtn = $e; break }
}
if (-not $overflowBtn) { Write-Output 'NOTFOUND no-overflow-button'; exit }

# 溢出面板是否已开（存在 TopLevelWindowForOverflowXamlIsland）
$flyout = $null
foreach ($cls in @('TopLevelWindowForOverflowXamlIsland', 'NotifyIconOverflowWindow')) {
  $f = Get-Tray $cls
  if ($f) { $flyout = $f; break }
}
if (-not $flyout) {
  $r = $overflowBtn.Current.BoundingRectangle
  Add-Type -AssemblyName System.Windows.Forms
  [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]($r.X + $r.Width/2), [int]($r.Y + $r.Height/2))
  Start-Sleep -Milliseconds 200
  # 左键点击（mouse_event）
  Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;
public class M { [DllImport("user32.dll")] public static extern void mouse_event(int f,int x,int y,int d,int e); }
"@
  [M]::mouse_event(0x0002,0,0,0,0); Start-Sleep -Milliseconds 60; [M]::mouse_event(0x0004,0,0,0,0)
  Start-Sleep -Milliseconds 1200
  foreach ($cls in @('TopLevelWindowForOverflowXamlIsland', 'NotifyIconOverflowWindow')) {
    $f = Get-Tray $cls
    if ($f) { $flyout = $f; break }
  }
}
if (-not $flyout) { Write-Output 'NOTFOUND flyout-not-shown'; exit }

# 2) 在溢出面板里找 Alya 图标
$items = $flyout.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$hit = $null
foreach ($e in $items) {
  $n = $e.Current.Name
  if ($n -match 'Alya') { $hit = $e; break }
}
if (-not $hit) {
  Write-Output ('NOTFOUND no-alya-icon; items=' + (($items | ForEach-Object { $_.Current.Name } | Where-Object { $_ } | Select-Object -First 12) -join ' / '))
  exit
}
$hr = $hit.Current.BoundingRectangle
Write-Output ('ICON ' + [int]($hr.X + $hr.Width/2) + ' ' + [int]($hr.Y + $hr.Height/2) + ' ' + $hit.Current.Name)
