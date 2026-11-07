# Checks examples/serial-echo on the board through a USB serial adapter on
# CN8, from Windows (PowerShell 5.1 or later, .NET's SerialPort):
#
#   powershell -File scripts/hw/serial-echo.ps1 [-Port COM8] [-Baud 115200]
#
# It also checks examples/rx74n-serial-echo on the EK-RX74N's J27, with -Port
# set to that board's port.
#
# Run it while the runner keeps the example running (after the runner
# detaches nothing comes out of the board yet; docs/NOTES-hal.md):
#   scripts/hw-build.sh examples                                    (WSL)
#   HW_TIMEOUT=60 scripts/hw-run.sh build/hw/examples/serial-echo.elf (Git Bash)
#
# Checks: a line comes back as sent; 4096 bytes (every byte value, 16 times)
# sent at full speed come back complete and in order (no overrun); a break
# is reported as `! framing error` and the echo goes on afterwards.
param([string]$Port = "COM8", [int]$Baud = 115200)

$sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, ([System.IO.Ports.Parity]::None), 8, ([System.IO.Ports.StopBits]::One)
$sp.Handshake = [System.IO.Ports.Handshake]::None
$sp.ReadTimeout = 200
$sp.WriteBufferSize = 8192
$sp.ReadBufferSize = 8192
$sp.Open()
$failed = 0

# Reads until $n bytes have arrived or nothing came for $quiet ms.
function Receive([int]$n, [int]$quiet = 1000) {
  $got = New-Object System.Collections.Generic.List[byte]
  $last = [Environment]::TickCount
  while ($got.Count -lt $n -and [Environment]::TickCount - $last -lt $quiet) {
    $k = $sp.BytesToRead
    if ($k -gt 0) {
      $b = New-Object byte[] $k
      $k = $sp.Read($b, 0, $k)
      $got.AddRange([byte[]]$b[0..($k - 1)])
      $last = [Environment]::TickCount
    } else {
      Start-Sleep -Milliseconds 5
    }
  }
  , $got.ToArray()
}

function Check([string]$name, [bool]$ok, [string]$detail = "") {
  if ($ok) { Write-Output "ok    $name" }
  else { Write-Output "FAIL  $name $detail"; $script:failed++ }
}

function Same([byte[]]$a, [byte[]]$b) {
  if ($a.Length -ne $b.Length) { return $false }
  for ($i = 0; $i -lt $a.Length; $i++) { if ($a[$i] -ne $b[$i]) { return $false } }
  $true
}

$ascii = [System.Text.Encoding]::ASCII
Start-Sleep -Milliseconds 100
$sp.DiscardInBuffer()

# 1. a line
$line = $ascii.GetBytes("hello from $Port`r`n")
$sp.Write($line, 0, $line.Length)
$got = Receive $line.Length
Check "a line comes back" (Same $line $got) ("got: " + $ascii.GetString($got))

# 2. 4096 bytes at full speed
$bulk = New-Object byte[] 4096
for ($i = 0; $i -lt $bulk.Length; $i++) { $bulk[$i] = [byte]($i % 256) }
$t0 = [Environment]::TickCount
$sp.Write($bulk, 0, $bulk.Length)
$got = Receive $bulk.Length
$ms = [Environment]::TickCount - $t0
$first = -1
for ($i = 0; $i -lt [Math]::Min($bulk.Length, $got.Length); $i++) {
  if ($bulk[$i] -ne $got[$i]) { $first = $i; break }
}
Check "4096 bytes come back in order ($ms ms)" (Same $bulk $got) ("got $($got.Length) bytes, first difference at $first")

# 3. a break: a framing error, then the echo goes on
$sp.DiscardInBuffer()
$sp.BreakState = $true
Start-Sleep -Milliseconds 20
$sp.BreakState = $false
$got = $ascii.GetString((Receive 4096 500))
Check "a break is a framing error" ($got -match "! framing error") ("got: " + $got.Trim())
$line = $ascii.GetBytes("after the break`r`n")
$sp.Write($line, 0, $line.Length)
$got = Receive $line.Length
Check "the echo goes on after the break" (Same $line $got) ("got: " + $ascii.GetString($got))

$sp.Close()
if ($failed) { Write-Output "== $failed failed"; exit 1 }
Write-Output "== all passed"
