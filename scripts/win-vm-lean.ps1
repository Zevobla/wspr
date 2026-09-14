<#
.SYNOPSIS
  Strip a Windows test VM down to a lean bench for whspr functional testing.

.DESCRIPTION
  Turns OFF the visual layer -- all animations, transitions, transparency,
  shadows, fades -- and a few background niceties, so the VM spends its cycles
  running whspr, not drawing chrome. It does NOT touch whspr or any dev tool;
  it only flips Windows' own eye-candy to "best performance".

  Run it INSIDE the Windows VM (this is a test-bench tweak, not part of the
  app build). No reboot needed -- it re-applies per-user parameters live and
  restarts Explorer at the end. Re-runnable and reversible (see -Restore).

.PARAMETER Restore
  Put the visual effects back to the Windows default ("let Windows choose").

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\win-vm-lean.ps1
  powershell -ExecutionPolicy Bypass -File .\win-vm-lean.ps1 -Restore
#>
[CmdletBinding()]
param(
  [switch]$Restore
)

$ErrorActionPreference = 'Stop'

function Set-Reg {
  param([string]$Path, [string]$Name, $Value, [string]$Type = 'DWord')
  if (-not (Test-Path $Path)) { New-Item -Path $Path -Force | Out-Null }
  New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType $Type -Force | Out-Null
  Write-Host ("  {0}\{1} = {2}" -f $Path, $Name, $Value)
}

$desktop  = 'HKCU:\Control Panel\Desktop'
$metrics  = 'HKCU:\Control Panel\Desktop\WindowMetrics'
$advanced = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
$vfx      = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects'
$dwm      = 'HKCU:\Software\Microsoft\Windows\DWM'
$personalize = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'

if ($Restore) {
  Write-Host "Restoring Windows visual effects to default..." -ForegroundColor Yellow
  Set-Reg $vfx 'VisualFXSetting' 0                 # 0 = let Windows choose
  Set-Reg $metrics 'MinAnimate' 1 'String'
  Set-Reg $advanced 'TaskbarAnimations' 1
  Set-Reg $dwm 'EnableAeroPeek' 1
  Set-Reg $personalize 'EnableTransparency' 1
  Set-Reg $desktop 'MenuShowDelay' 400 'String'
} else {
  Write-Host "Stripping the Windows visual layer (lean test bench)..." -ForegroundColor Cyan

  # Master switch: "Adjust for best performance" -> disables the whole
  # animation/shadow/fade set the VFX control panel governs.
  Set-Reg $vfx 'VisualFXSetting' 2

  # UserPreferencesMask is the packed bitfield behind that control panel. This
  # value is what "best performance" writes: every UI animation/effect off,
  # while keeping fonts readable. (Bytes: anim off, combo/menu/tooltip fades
  # off, drag-full-windows kept for usable window moves.)
  Set-Reg $desktop 'UserPreferencesMask' ([byte[]](0x90,0x12,0x03,0x80,0x10,0x00,0x00,0x00)) 'Binary'

  # Individual animation/effect toggles (belt-and-suspenders; some apps read
  # these directly rather than the mask).
  Set-Reg $metrics 'MinAnimate' 0 'String'          # minimize/maximize animation
  Set-Reg $advanced 'TaskbarAnimations' 0           # taskbar animations
  Set-Reg $advanced 'ListviewAlphaSelect' 0         # translucent selection rect
  Set-Reg $advanced 'ListviewShadow' 0              # icon-label drop shadows
  Set-Reg $dwm 'EnableAeroPeek' 0                    # Aero Peek
  Set-Reg $personalize 'EnableTransparency' 0        # taskbar/menu transparency
  Set-Reg $desktop 'MenuShowDelay' 0 'String'        # instant menus
  Set-Reg $desktop 'DragFullWindows' 1 'String'      # keep drag usable, no ghost

  # Faster shell startup + no background app churn on a bench.
  Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize' 'StartupDelayInMSec' 0
  $bg = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications'
  Set-Reg $bg 'GlobalUserDisabled' 1

  # High-performance power plan so the VM never clocks down mid-test.
  try { powercfg /setactive SCHEME_MIN 2>$null; Write-Host "  power plan -> High performance" } catch {}
}

# Apply per-user parameters live (no logout) and refresh the shell.
Write-Host "Applying..." -ForegroundColor Cyan
rundll32.exe user32.dll,UpdatePerUserSystemParameters ,1 ,True
Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Process explorer.exe

Write-Host "Done." -ForegroundColor Green
