. "$PSScriptRoot/environment.ps1"
Add-Type -AssemblyName System.Drawing
$taskBitmap = [Drawing.Bitmap]::new(64,64)
$taskGraphics = [Drawing.Graphics]::FromImage($taskBitmap)
$taskGraphics.Clear([Drawing.Color]::FromArgb(16,16,16))
$taskRed = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(239,63,59))
$taskWhite = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(232,229,217))
foreach ($taskCell in @(@(3,1),@(4,1),@(3,2),@(4,2),@(1,3),@(2,3),@(3,3),@(4,3),@(5,3),@(6,3),@(2,4),@(3,4),@(4,4),@(5,4),@(3,5),@(4,5))) {
    $taskGraphics.FillRectangle($taskRed,[int]($taskCell[0]*8+1),[int]($taskCell[1]*8+1),6,6)
}
foreach ($taskCell in @(@(1,6),@(2,6),@(3,6),@(4,6),@(5,6),@(6,6))) {
    $taskGraphics.FillRectangle($taskWhite,[int]($taskCell[0]*8+1),[int]($taskCell[1]*8+1),6,6)
}
$taskIcon = [Drawing.Icon]::FromHandle($taskBitmap.GetHicon())
$taskAssets = Join-Path $taskRoot 'apps/windows/assets'
New-Item -ItemType Directory -Force -Path $taskAssets | Out-Null
$taskStream = [IO.File]::Create((Join-Path $taskAssets 'drop.ico'))
try { $taskIcon.Save($taskStream) } finally { $taskStream.Dispose();$taskIcon.Dispose();$taskGraphics.Dispose();$taskBitmap.Dispose();$taskRed.Dispose();$taskWhite.Dispose() }
