# Starts the title with the settings this fork is measured with (README.md, "How to run it").
#   scripts\play.ps1 -Game <path of your own copy's executable> [-Cache <dir>] [-Data <dir>] [-Safe] [-Build diag|release]
# -Safe   without the newest switches (try it first when something is wrong)
# Close the game window to quit. The log goes to play_last.txt next to this script.
param(
    [Parameter(Mandatory)][string]$Game,
    [string]$Cache = "$PSScriptRoot\..\cache",
    [string]$Data = "$PSScriptRoot\..\data",
    [switch]$Safe,
    [ValidateSet('diag', 'release')][string]$Build = 'diag'
)
$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; return }
if (-not (Test-Path $Game)) { Write-Host "No $Game."; return }
New-Item -ItemType Directory -Force $Cache, $Data | Out-Null

$settings = @{
    VICTORY_CACHE_DIR = (Resolve-Path $Cache).Path; VICTORY_DATA_DIR = (Resolve-Path $Data).Path
    # the base set
    VICTORY_UPSCALER = 'spatial'; VICTORY_WIDE_COPIES = '1'; VICTORY_ASYNC_SUBMIT = '1'; VICTORY_QUEUE2 = '1'
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_JOIN_PARTS = '1'; VICTORY_BACK_OWN_VIEW = '1'
    VICTORY_RELAY = '2'; VICTORY_LATE_WAIT = '3'; VICTORY_CARD_BUFFERS_MIN_MB = '1'; VICTORY_CARD_BUFFERS_MB = '1024'
    VICTORY_INDIRECT_WINDOW = '1'; VICTORY_WIDE_MERGE = '1'
    # light
    VICTORY_AMBIENT = '0.6'; VICTORY_AMBIENT_REFLECT = '0.5'; VICTORY_AMBIENT_SKY = '1'
    # textures that fit a 6-8 GB card
    VICTORY_TEXTURE_MIP_DROP = '2'; VICTORY_TEXTURE_MIP_DROP_FLOOR = '128'; VICTORY_TEXTURE_MIP_DROP_COLOR = '1'
    VICTORY_DETILE_ZERO = '1'
    # stability
    VICTORY_STEP_OVER = '2215c35,23cf683'
    VICTORY_WAVE_WIDE_ONLY = '369cbe906ea72764,2cc4c197ee29e291,5dc3b25986622949,6f225b641c5b0708'
}
$newest = @{
    VICTORY_COPY_PLAIN = '1'; VICTORY_COPY_PLAIN_LEAST = '32'; VICTORY_ASYNC_PRESENT = '1'; VICTORY_FRAMES_AHEAD = '2'
    VICTORY_CARD_HASH_WAVE = '1'; VICTORY_MERGE_MASKED = '1'; VICTORY_MERGE_MASKED_INDEX = '1'
    VICTORY_MERGE_BUFFERS = '1'; VICTORY_MERGE_CHUNKS = '1'
    VICTORY_WATCH_TEXTURES_JOINED = '1'; VICTORY_WATCH_VIEWS_SKIP = '1'
    VICTORY_FILL_AHEAD_COPY_MISS = '1'; VICTORY_COPY_AFTER_LANDING = '1'; VICTORY_COPY_AHEAD = '1'
    VICTORY_GUARD_ABSENT_CACHE = '4'; VICTORY_GUARD_REST = '1'; VICTORY_OPEN_ONE_VIEW = '1'; VICTORY_GUARD_BITS = '1'
    VICTORY_FINE_PAGES = '1'; VICTORY_MIRROR_FAST = '1'; VICTORY_CLZERO_REGIONS = '64'
    VICTORY_JOURNAL_SPLIT = '1'; VICTORY_WIDE_REFRESH = '1'
    VICTORY_TEXTURE_MIP_DROP_UI_FULL = '2'; VICTORY_KEEP_SHOWN = '1'; VICTORY_SHOW_AFTER = '1'
    VICTORY_BC7_ROTATION = '1'; VICTORY_MESH_9BIT_CORNERS = '1'
}
if (-not $Safe) { foreach ($k in $newest.Keys) { $settings[$k] = $newest[$k] } }

# Settings left over in this window go.
Get-ChildItem Env:VICTORY_* | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$log = Join-Path $PSScriptRoot 'play_last.txt'
& $victory run (Resolve-Path $Game).Path --execute --console series-s *> $log
Write-Host "Game closed. Log: $log"
