param(
    [switch]$SelfTest,
    [switch]$Once,
    [switch]$TestAlert,
    [string]$NekorayDirectory = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$script:health = @{ Failures = 0; Successes = 0; Alerted = $false }

function Update-HealthState {
    param([hashtable]$State, [bool]$Healthy)
    if ($Healthy) {
        $State.Failures = 0
        $State.Successes++
        if ($State.Alerted -and $State.Successes -ge 2) {
            $State.Alerted = $false
            return 'recovered'
        }
    } else {
        $State.Successes = 0
        $State.Failures++
        if (-not $State.Alerted -and $State.Failures -ge 3) {
            $State.Alerted = $true
            return 'disconnected'
        }
    }
    return 'none'
}

if ($SelfTest) {
    $state = @{ Failures = 0; Successes = 0; Alerted = $false }
    $inputs = @($true,$false,$false,$true,$false,$false,$false,$false,$true,$false,$true,$true,$false,$false,$false)
    $expected = @('none','none','none','none','none','none','disconnected','none','none','none','none','recovered','none','none','disconnected')
    for ($i = 0; $i -lt $inputs.Count; $i++) {
        $actual = Update-HealthState $state $inputs[$i]
        if ($actual -ne $expected[$i]) { throw "State transition $i failed: $actual" }
    }
    if (-not $state.Alerted) { throw 'Second incident was not detected' }
    Write-Output 'PASS: transient failure, three-failure threshold, no repeated alerts, interrupted recovery, two-success recovery, second incident'
    exit 0
}

$script:curlPath = Join-Path $env:SystemRoot 'System32\curl.exe'
$script:endpoints = @('https://www.gstatic.com/generate_204', 'https://cp.cloudflare.com/generate_204')
$script:logPath = Join-Path $PSScriptRoot 'nekoray-monitor.log'
$script:logError = $false

function Start-ChannelRequest {
    param($Channel)
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $script:curlPath
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($name in @('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY')) {
        $info.EnvironmentVariables.Remove($name)
    }
    $proxyArgs = '--proxy "" --noproxy "*"'
    if ($Channel.Proxy) {
        $proxyArgs = '--proxy "' + $Channel.Proxy + '" --noproxy "localhost,127.0.0.1"'
    }
    $info.Arguments = '--disable --silent --show-error --connect-timeout 4 --max-time 8 --output NUL --write-out "%{http_code}" ' + $proxyArgs + ' "' + $script:endpoints[$Channel.Endpoint] + '"'
    $Channel.Process = New-Object System.Diagnostics.Process
    $Channel.Process.StartInfo = $info
    try {
        [void]$Channel.Process.Start()
        $Channel.Deadline = [DateTime]::UtcNow.AddSeconds(10)
    } catch {
        $Channel.Process.Dispose()
        $Channel.Process = $null
        $Channel.Code = 'curl_start_failed'
        $Channel.Done = $true
    }
}

function Start-ProbeSet {
    foreach ($definition in @(
        @{ Name = 'SOCKS'; Proxy = 'socks5h://127.0.0.1:2080' },
        @{ Name = 'HTTP'; Proxy = 'http://127.0.0.1:2081' },
        @{ Name = 'OS_TUN'; Proxy = $null }
    )) {
        $channel = @{ Name = $definition.Name; Proxy = $definition.Proxy; Endpoint = 0; Done = $false; OK = $false; Code = ''; Process = $null; Deadline = [DateTime]::UtcNow }
        Start-ChannelRequest $channel
        Write-Output $channel
    }
}

function Update-ProbeSet {
    param([array]$Channels)
    foreach ($channel in $Channels) {
        if ($channel.Done) { continue }
        $process = $channel.Process
        if (-not $process.HasExited -and [DateTime]::UtcNow -lt $channel.Deadline) { continue }
        if (-not $process.HasExited) {
            try { $process.Kill() } catch { }
            $channel.Code = 'watchdog_timeout'
            # A killed child is reaped on a later tick; the UI thread must not wait.
            continue
        }
        $httpCode = $process.StandardOutput.ReadToEnd().Trim()
        [void]$process.StandardError.ReadToEnd()
        $exitCode = $process.ExitCode
        $process.Dispose()
        $channel.Process = $null
        $channel.OK = ($exitCode -eq 0 -and $httpCode -eq '204')
        $channel.Code = 'curl=' + $exitCode + ';http=' + $httpCode
        if (-not $channel.OK -and $channel.Endpoint -eq 0) {
            $channel.Endpoint = 1
            Start-ChannelRequest $channel
        } else {
            $channel.Done = $true
        }
    }
    return (@($Channels | Where-Object { -not $_.Done }).Count -eq 0)
}

function Get-ConnectionSnapshot {
    param([array]$Channels)
    $processNames = @(Get-Process -Name 'nekoray','nekoray_core','nekobox_core' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty ProcessName)
    $tunUp = @([System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object { $_.Name -eq 'nekoray-tun' -and $_.OperationalStatus -eq 'Up' }).Count -gt 0
    $coreUp = ($processNames -contains 'nekoray_core') -and ($processNames -contains 'nekobox_core') -and ($processNames -contains 'nekoray')
    $results = @{}
    foreach ($channel in $Channels) { $results[$channel.Name] = [bool]$channel.OK }
    $failed = @($Channels | Where-Object { -not $_.OK } | ForEach-Object { $_.Name })
    $reason = 'Все проверяемые пути отвечают.'
    if (-not $coreUp) {
        $reason = 'Не найден Nekoray или одно из его ядер.'
    } elseif (-not $tunUp) {
        $reason = 'Адаптер nekoray-tun выключен или недоступен.'
    } elseif ($results.SOCKS -and $results.HTTP -and -not $results.OS_TUN) {
        $reason = 'Локальные прокси отвечают, но путь ОС/TUN не отвечает. Проверьте TUN, DNS и маршруты.'
    } elseif ($failed.Count -gt 0) {
        $reason = 'Не отвечают: ' + ($failed -join ', ') + '. Причина может быть в прокси, сервере или сети.'
    }
    $proxySettings = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
    [pscustomobject]@{
        Time = [DateTime]::Now.ToString('yyyy-MM-dd HH:mm:ss zzz')
        Healthy = ($coreUp -and $tunUp -and $failed.Count -eq 0)
        ProcessesPresent = $coreUp
        TunAdapterUp = $tunUp
        SystemProxyEnabled = ($proxySettings.ProxyEnable -eq 1)
        Results = $results
        Codes = @($Channels | ForEach-Object { $_.Name + ':' + $_.Code + ';endpoint=' + $_.Endpoint })
        Reason = $reason
    }
}

function Write-MonitorLog {
    param([string]$Event, $Snapshot)
    try {
        if ((Test-Path -LiteralPath $script:logPath) -and (Get-Item -LiteralPath $script:logPath).Length -gt 2MB) {
            [System.IO.File]::Copy($script:logPath, ($script:logPath + '.previous'), $true)
            [System.IO.File]::WriteAllText($script:logPath, '')
        }
        $entry = [pscustomobject]@{ Event = $Event; Snapshot = $Snapshot } | ConvertTo-Json -Depth 5 -Compress
        [System.IO.File]::AppendAllText($script:logPath, ($entry + [Environment]::NewLine), (New-Object System.Text.UTF8Encoding($false)))
    } catch {
        $script:logError = $true
    }
}

if (-not $TestAlert) {
    if (-not (Test-Path -LiteralPath $script:curlPath)) { throw 'Windows curl.exe was not found' }
    if (-not (Test-Path -LiteralPath (Join-Path $NekorayDirectory 'nekoray.exe'))) { throw 'Nekoray directory is invalid; use -NekorayDirectory' }
}

if ($Once) {
    $channels = @(Start-ProbeSet)
    try {
        while (-not (Update-ProbeSet $channels)) { Start-Sleep -Milliseconds 100 }
        Get-ConnectionSnapshot $channels | ConvertTo-Json -Depth 5
    } finally {
        foreach ($channel in $channels) {
            if ($channel.Process) {
                try { if (-not $channel.Process.HasExited) { $channel.Process.Kill() } } catch { }
                $channel.Process.Dispose()
            }
        }
    }
    exit 0
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
$mutex = New-Object System.Threading.Mutex($false, 'Local\NekorayTrafficMonitor')
$ownsMutex = $false
if (-not $TestAlert) {
    try { $ownsMutex = $mutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $ownsMutex = $true }
    if (-not $ownsMutex) { $mutex.Dispose(); exit 0 }
}

$form = New-Object System.Windows.Forms.Form
$form.Text = 'Nekoray — монитор соединения'
$form.ClientSize = New-Object System.Drawing.Size(570, 240)
$form.StartPosition = 'CenterScreen'
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.MinimizeBox = $false
$form.TopMost = $true
$form.Font = New-Object System.Drawing.Font('Segoe UI', 10)
$heading = New-Object System.Windows.Forms.Label
$heading.Location = New-Object System.Drawing.Point(20, 18)
$heading.Size = New-Object System.Drawing.Size(530, 32)
$heading.Font = New-Object System.Drawing.Font('Segoe UI', 14, [System.Drawing.FontStyle]::Bold)
$heading.Text = 'Проверка соединения...'
$details = New-Object System.Windows.Forms.Label
$details.Location = New-Object System.Drawing.Point(20, 58)
$details.Size = New-Object System.Drawing.Size(530, 125)
$details.Text = 'Проверяются SOCKS, HTTP-прокси и путь ОС/TUN.'
$hideButton = New-Object System.Windows.Forms.Button
$hideButton.Text = 'Скрыть'
$hideButton.Location = New-Object System.Drawing.Point(20, 195)
$hideButton.Size = New-Object System.Drawing.Size(130, 30)
$stopButton = New-Object System.Windows.Forms.Button
$stopButton.Text = 'Остановить монитор'
$stopButton.Location = New-Object System.Drawing.Point(340, 195)
$stopButton.Size = New-Object System.Drawing.Size(210, 30)
$form.Controls.AddRange(@($heading,$details,$hideButton,$stopButton))
$tray = New-Object System.Windows.Forms.NotifyIcon
$tray.Icon = [System.Drawing.SystemIcons]::Information
$tray.Text = 'Nekoray: проверка соединения'
$tray.Visible = $true
$menu = New-Object System.Windows.Forms.ContextMenuStrip
$showItem = $menu.Items.Add('Показать состояние')
$exitItem = $menu.Items.Add('Остановить монитор')
$tray.ContextMenuStrip = $menu
$script:exitRequested = $false
$hideButton.Add_Click({ $form.Hide() })
$showWindow = { $form.Show(); $form.BringToFront() }
$showItem.Add_Click($showWindow)
$tray.Add_DoubleClick($showWindow)
$stopMonitor = { $script:exitRequested = $true; $form.Close(); [System.Windows.Forms.Application]::ExitThread() }
$stopButton.Add_Click($stopMonitor)
$exitItem.Add_Click($stopMonitor)
$form.Add_FormClosing({
    param($sender, $eventArgs)
    if (-not $script:exitRequested -and -not $TestAlert -and $eventArgs.CloseReason -eq 'UserClosing') {
        $eventArgs.Cancel = $true
        $form.Hide()
    }
})

$script:channels = $null
$script:nextRound = [DateTime]::UtcNow
$script:lastHeartbeat = [DateTime]::MinValue
$script:lastSnapshot = $null
$previewEnd = [DateTime]::UtcNow.AddSeconds(12)
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 250
$timer.Add_Tick({
    try {
        if ($TestAlert) {
            if ([DateTime]::UtcNow -ge $previewEnd) { & $stopMonitor }
            return
        }
        if ($null -eq $script:channels) {
            if ([DateTime]::UtcNow -ge $script:nextRound) {
                $script:nextRound = [DateTime]::UtcNow.AddSeconds(15)
                $script:channels = @(Start-ProbeSet)
            }
            return
        }
        if (-not (Update-ProbeSet $script:channels)) { return }
        $snapshot = Get-ConnectionSnapshot $script:channels
        $script:lastSnapshot = $snapshot
        $script:channels = $null
        $transition = Update-HealthState $script:health $snapshot.Healthy
        $heading.Text = if ($snapshot.Healthy) { 'Соединение отвечает' } else { 'Проблема соединения Nekoray' }
        $details.Text = $snapshot.Reason + "`r`n`r`nПроверено: " + $snapshot.Time + "`r`nНеудачных раундов подряд: " + $script:health.Failures
        $tray.Text = if ($snapshot.Healthy) { 'Nekoray: соединение отвечает' } else { 'Nekoray: ошибка соединения' }
        $tray.Icon = if ($snapshot.Healthy) { [System.Drawing.SystemIcons]::Information } else { [System.Drawing.SystemIcons]::Error }
        if (-not $snapshot.Healthy -or $transition -ne 'none' -or [DateTime]::UtcNow -ge $script:lastHeartbeat.AddMinutes(5)) {
            Write-MonitorLog $transition $snapshot
            $script:lastHeartbeat = [DateTime]::UtcNow
        }
        if ($script:logError) { $details.Text += "`r`nВнимание: журнал не записывается." }
        if ($transition -eq 'disconnected') {
            $form.Show()
            $form.BringToFront()
            [System.Media.SystemSounds]::Exclamation.Play()
        } elseif ($transition -eq 'recovered') {
            $form.Hide()
            $tray.ShowBalloonTip(3000, 'Nekoray', 'Соединение восстановилось.', [System.Windows.Forms.ToolTipIcon]::Info)
        }
    } catch {
        $heading.Text = 'Ошибка самого монитора'
        $details.Text = 'Проверка остановлена из-за внутренней ошибки. Перезапустите монитор. Код: ' + $_.Exception.GetType().Name
        Write-MonitorLog 'monitor_error' ([pscustomobject]@{ Time=[DateTime]::Now.ToString('o'); Error=$_.Exception.GetType().Name })
        $timer.Stop()
        $form.Show()
        $form.BringToFront()
    }
})

try {
    if ($TestAlert) {
        $heading.Text = 'ПРОВЕРКА: предупреждение Nekoray'
        $details.Text = 'Это тестовое окно поверх обычных приложений. Сеть и настройки не изменяются. Окно закроется через 12 секунд.'
        $form.Show()
        [System.Media.SystemSounds]::Exclamation.Play()
    } else {
        Write-MonitorLog 'started' ([pscustomobject]@{ Time=[DateTime]::Now.ToString('o'); IntervalSeconds=15; FailureThreshold=3; RecoveryThreshold=2 })
        $tray.ShowBalloonTip(3000, 'Монитор Nekoray запущен', 'При устойчивом отказе появится окно поверх приложений. Выход — через значок в трее.', [System.Windows.Forms.ToolTipIcon]::Info)
    }
    $timer.Start()
    [System.Windows.Forms.Application]::Run()
} finally {
    $timer.Stop()
    $timer.Dispose()
    foreach ($channel in $script:channels) {
        if ($channel.Process) {
            try { if (-not $channel.Process.HasExited) { $channel.Process.Kill() } } catch { }
            $channel.Process.Dispose()
        }
    }
    if (-not $TestAlert) { Write-MonitorLog 'stopped' ([pscustomobject]@{ Time=[DateTime]::Now.ToString('o') }) }
    $tray.Visible = $false
    $tray.Dispose()
    $menu.Dispose()
    $form.Dispose()
    if ($ownsMutex) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
