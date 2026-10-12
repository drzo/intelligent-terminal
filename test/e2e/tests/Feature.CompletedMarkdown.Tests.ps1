#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }
<#
Deterministic TDD matrix. Baseline: completed replies still show **markers**.
The normal stdio ACP -> master -> helper -> Terminal render/copy boundary is real;
the agent is a fixture, never a provider. No private test kit is imported.

| Contract | Trigger | Boundary | Oracle | Negative control | Existing protection | Checklist title |
| --- | --- | --- | --- | --- | --- | --- |
| Raw throughout an active turn; batch only after normal end | Prefix, tail and finish gates | ACP notifications/end -> completed display | Literal prefix and received tail before finish, then marker-free text and completed_markdown DEBUG work | Held finish has zero work, including after tail | AgentProtocolExperience; WTA completed_markdown units | Completed replies render Markdown after raw streaming |
| Only normally completed agent segments format, without moving other messages | Agent/tool failure/thought/agent, then normal end; separate cancel and RPC-error turns | Mixed transcript and lifecycle -> visible history | Ordered independent formatted segments; user/tool/thought/error text stays literal; interrupted segments never render | Tool failure is not turn failure; cancellation and RPC failure are | AgentProtocolExperience; PromptHistory | Interrupted replies stay raw and preserve transcript order |
| Cached history shares draw, wrap and copy geometry | Collapse/expand, narrow/wide owned window, wheel, ConPTY mouse selection and native copy | Completed cache -> Ratatui viewport -> exact UIA anchor and verified single-cell prefix -> ConPTY input -> clipboard | Table and wrap anchors, exact visible-text copy, width-dependent reflow, unchanged work on settled redraws and stable identities | No work from unchanged frames or history navigation | CompletedTurnSelection; AgentMouse CompletedTurnMouse; MouseInput units | Completed Markdown keeps cached history and selection geometry |
| Selection pins the raw snapshot, not already formatted history | ConPTY drag before finish, then release finish and copy | Mouse selection -> completion -> copied cells -> first Markdown display | Raw copy after normal end; formatted old history unchanged; clear/copy permits transition | Normal completion without selection transitions immediately | AgentMouse; AgentSelectAll; OwnedLaunchPointerSafety units | Selecting streamed text defers completed Markdown |

Run only with ITE2E_PACKAGE=Dev and exact-source ITE2E_EXPECTED_APP_SHA256
(TerminalApp.dll) / ITE2E_EXPECTED_WTA_SHA256 from the build receipt.
ITE2E_ARTIFACT_ROOT must be explicitly supplied for this run. Start-Terminal
owns backup/recovery; Stop-Terminal is the only process teardown.

Fixture gates: -CompletedMarkdownGateDirectory <case directory>;
CM_<STREAM|ORDER|CANCEL|ERROR|HISTORY>_<12 lowercase hex> selects a synthetic turn.
<id>.tail releases the remaining source; <id>.finish releases normal end (ERROR
returns an RPC error); abort releases a pending turn during teardown. Cancellation
is a real session/cancel, not a gate. Every held turn has a 120-second deadline.

The released tui-markdown renderer retains heading/fence symbols. Strong emphasis
and a small table are the oracles, not hidden heading symbols or syntax coloring.
Only completed_markdown DEBUG "Completed Markdown batch render" records are
counted; no diagnostic field layout, private instrumentation or incremental API
is assumed. The matching positive completion prevents a disabled logger from
passing the zero-streaming-work assertion.
Native input must use the public helpers, including their fail-closed refusals.
Mouse selection uses the existing public ConPTY event helper; it does not bypass
the framework's disabled native drag or claim OS pointer-translation coverage.
#>

Describe 'Feature: completed Markdown' -Tag @('Feature', 'CompletedMarkdown') {
    BeforeAll {
        Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
        if (-not [string]::Equals((Get-ItTestPackage), 'Dev', [StringComparison]::Ordinal)) {
            throw 'Completed Markdown requires an explicitly selected Debug Dev package.'
        }
        foreach ($name in 'ITE2E_EXPECTED_APP_SHA256', 'ITE2E_EXPECTED_WTA_SHA256') {
            if ([Environment]::GetEnvironmentVariable($name) -notmatch '^[0-9a-fA-F]{64}$') {
                throw "Supply $name from the exact-source build receipt, not the installed binary."
            }
        }
        if (-not $env:ITE2E_ARTIFACT_ROOT) { throw 'Supply a unique ITE2E_ARTIFACT_ROOT for this run.' }
        $script:cmRoot = Join-Path ([IO.Path]::GetFullPath($env:ITE2E_ARTIFACT_ROOT)) (
            'completed-markdown-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:cmRoot | Out-Null
        $script:cmCaseNumber = 0
        $script:cmReady = Get-WtaLocalizedTextRegex -Key 'input.placeholder.connected'
        $script:cmReady | Should -Not -BeNullOrEmpty

        function Read-CmPane {
            Get-AgentPaneText -App $script:cmApp -PaneSessionId $script:cmPane -MaxLines 500
        }

        function Wait-CmPane {
            param([string[]]$Present = @(), [string[]]$Absent = @(), [switch]$Completed)
            Wait-Until -TimeoutSec 15 -IntervalSec 0.2 -Because 'the gated completed-Markdown display contract' -Condition {
                $text = Read-CmPane
                foreach ($value in $Present) {
                    if (-not $text.Contains($value, [StringComparison]::Ordinal)) { return $false }
                }
                foreach ($value in $Absent) {
                    if ($text.Contains($value, [StringComparison]::Ordinal)) { return $false }
                }
                if ($Completed -and $text -notmatch $script:cmReady) { return $false }
                $text
            }
        }

        function Wait-CmFixture {
            param($Turn, [string]$Event)
            $needle = "|markdown|$($Turn.Id)|$Event"
            Test-Until -TimeoutSec 10 -IntervalSec 0.1 -Condition {
                (Test-Path -LiteralPath $script:cmFixtureLog) -and
                    (Get-Content -LiteralPath $script:cmFixtureLog -Raw).Contains($needle, [StringComparison]::Ordinal)
            } | Should -BeTrue -Because "the fixture must acknowledge $Event for this exact turn"
        }

        function Start-CmTurn {
            param([ValidateSet('STREAM', 'ORDER', 'CANCEL', 'ERROR', 'HISTORY')][string]$Mode = 'STREAM', [int]$Rows = 12)
            $id = [guid]::NewGuid().ToString('N').Substring(0, 12)
            $turn = [pscustomobject]@{
                Id = $id; Prefix = "CM_${id}_PREFIX"; Tail = "CM_${id}_TAIL"
                After = "CM_${id}_AFTER"; User = "**CM_${id}_USER**"
                Cell = "CM_${id}_CELL"; WrapStart = "CM_${id}_WRAP_START"; WrapEnd = "CM_${id}_WRAP_END"
            }
            Send-AgentPrompt -App $script:cmApp -PaneSessionId $script:cmPane `
                -Text "CM_${Mode}_$id $($turn.User) ROWS=$Rows" | Out-Null
            Wait-CmFixture -Turn $turn -Event prefix
            $turn
        }

        function Release-CmGate {
            param($Turn, [ValidateSet('tail', 'finish')][string]$Gate)
            Set-Content -LiteralPath (Join-Path $script:cmGates "$($Turn.Id).$Gate") -Value 'release' -Encoding utf8NoBOM
            if ($Gate -eq 'tail') { Wait-CmFixture -Turn $Turn -Event tail }
        }

        function Get-CmWork {
            $log = Get-ItLogText -App $script:cmApp -Name "wta-main_helper-$($script:cmSession.HelperProcessId).log" -SinceStart
            @($log -split '\r?\n' | Where-Object {
                $_ -match '\bDEBUG\b.*\bcompleted_markdown\b' -and
                    $_.Contains('Completed Markdown batch render', [StringComparison]::Ordinal)
            })
        }

        function Assert-CmWorkStable {
            param([int]$Count)
            # The held source/finish or completed placeholder is established first.
            # This bounded observation is not a sleep standing in for readiness.
            $clock = [Diagnostics.Stopwatch]::StartNew()
            do {
                $null = Read-CmPane
                $work = @(Get-CmWork)
                $work.Count | Should -Be $Count -Because 'streaming and unchanged frames must not do batch work'
                ($work -join "`n") | Should -Not -Match 'CM_[0-9a-f]{12}_' -Because 'batch diagnostics contain metadata, not transcript source'
                Start-Sleep -Milliseconds 200
            } while ($clock.Elapsed.TotalSeconds -lt 1.2)
        }

        function Wait-CmWork {
            param([int]$Count)
            Test-Until -TimeoutSec 10 -IntervalSec 0.2 -Condition { @(Get-CmWork).Count -eq $Count } |
                Should -BeTrue -Because 'enabled DEBUG diagnostics must prove one batch per independent completed agent segment'
            Assert-CmWorkStable -Count $Count
        }

        function Save-CmEvidence {
            param([string]$Name)
            Read-CmPane | Set-Content -LiteralPath (Join-Path $script:cmEvidence "$Name.txt") -Encoding utf8NoBOM
            @(Get-CmWork) | Set-Content -LiteralPath (Join-Path $script:cmEvidence "$Name-work.log") -Encoding utf8NoBOM
            Save-UiScreenshot -App $script:cmApp -Path (Join-Path $script:cmEvidence "$Name.png") | Out-Null
        }

        function Get-CmIdentity {
            $session = Get-AgentPaneSession -App $script:cmApp -PaneSessionId $script:cmPane
            $ids = @(Get-DescendantWtaIds -RootPid $script:cmApp.Pid)
            $masters = @(Get-CimInstance Win32_Process -Filter "Name='wta.exe'" | Where-Object {
                [int]$_.ProcessId -in $ids -and $_.CommandLine -match '(?:^|\s)--master\s'
            })
            $masters | Should -HaveCount 1
            $terminal = Get-Process -Id $script:cmApp.Pid -ErrorAction Stop
            $helper = Get-Process -Id $session.HelperProcessId -ErrorAction Stop
            [ordered]@{
                Terminal = $terminal.Id; TerminalStart = $terminal.StartTime.ToUniversalTime().Ticks
                Helper = $helper.Id; HelperStart = $helper.StartTime.ToUniversalTime().Ticks
                Master = $masters[0].ProcessId; MasterStart = $masters[0].CreationDate.ToUniversalTime().Ticks
                Pane = $session.PaneSessionId; Session = $session.AcpSessionId
            } | ConvertTo-Json -Compress
        }

        function Assert-CmIdentity {
            [string]::Equals((Get-CmIdentity), $script:cmIdentity, [StringComparison]::Ordinal) |
                Should -BeTrue -Because 'formatting, navigation and copying must not replace the app, helper, master or ACP session'
        }

        function Get-CmGeometry {
            $text = Read-CmPane
            $lines = @($text.TrimEnd("`r", "`n") -split '\r?\n')
            # Only known single-cell box-drawing chrome measures columns. UTF-16
            # offsets in transcript content are never treated as display cells.
            $borders = @($lines | ForEach-Object {
                [regex]::Match($_, '[\u2500-\u257f]{8,}').Length
            })
            $columns = [int]($borders | Measure-Object -Maximum).Maximum
            if ($columns -lt 40) { throw 'The current capture does not expose a usable full-width input border.' }
            [pscustomobject]@{ Rows = $lines.Count; Columns = $columns; Lines = $lines }
        }

        function Select-CmVisibleText {
            param([string]$Text)
            if ($Text -notmatch '^[\x20-\x7e]+$') { throw 'The selection anchor must be single-cell ASCII.' }
            Set-AgentPaneFocus -App $script:cmApp | Out-Null
            Set-WtWindowForeground -App $script:cmApp | Should -BeTrue
            $rectangles = @(Get-UiTextBounds -App $script:cmApp -Text $Text)
            $rectangles | Should -HaveCount 1 -Because 'selection must target one exact visible row, not a guessed cell offset'
            $rect = $rectangles[0]
            $cell = $rect.Width / $Text.Length
            $cell | Should -BeGreaterThan 0
            $lines = (Get-CmGeometry).Lines
            $hits = @(for ($row = 0; $row -lt $lines.Count; $row++) {
                if ($lines[$row].Contains($Text, [StringComparison]::Ordinal)) { $row }
            })
            $hits | Should -HaveCount 1
            $line = $lines[$hits[0]]
            $prefix = $line.Substring(0, $line.IndexOf($Text, [StringComparison]::Ordinal))
            if ($prefix -notmatch '^[\x20-\x7e\u2500-\u257f\u2022\u25cf]*$') {
                throw 'The fixture selection prefix must contain only single-cell ASCII, borders or reply markers.'
            }
            $column = $prefix.Length
            Send-AgentMouseEvent -App $script:cmApp -PaneSessionId $script:cmPane -Kind Down -Column $column -Row $hits[0] | Out-Null
            Send-AgentMouseEvent -App $script:cmApp -PaneSessionId $script:cmPane -Kind Drag -Column ($column + $Text.Length - 1) -Row $hits[0] | Out-Null
            Send-AgentMouseEvent -App $script:cmApp -PaneSessionId $script:cmPane -Kind Up -Column ($column + $Text.Length - 1) -Row $hits[0] | Out-Null
        }

        function Copy-CmSelection {
            param([string]$Expected)
            Set-Clipboard -Value "CM_CLIPBOARD_$([guid]::NewGuid().ToString('N'))"
            Send-WtWindowKey -App $script:cmApp -Vk 0x43 -Ctrl -RequireForeground | Out-Null
            Test-Until -TimeoutSec 8 -IntervalSec 0.2 -Condition {
                [string]::Equals((Get-Clipboard -Raw), $Expected, [StringComparison]::Ordinal)
            } | Should -BeTrue -Because 'physical Ctrl+C must copy exactly the selected rendered cells, including any literal markers'
        }

        function Invoke-CmTranscriptHeader {
            param([string]$Text)
            if ($Text -notmatch '^[\x20-\x7e]+$') { throw 'The transcript header anchor must be ASCII.' }
            $lines = (Get-CmGeometry).Lines
            $hits = @(for ($row = 0; $row -lt $lines.Count; $row++) {
                if ($lines[$row].Contains($Text, [StringComparison]::Ordinal)) { $row }
            })
            $hits | Should -HaveCount 1
            $label = @(Get-UiTextBounds -App $script:cmApp -Text $Text)
            $line = @(Get-UiTextBounds -App $script:cmApp -Text $lines[$hits[0]].TrimEnd())
            $label | Should -HaveCount 1
            $line | Should -HaveCount 1
            $cell = $label[0].Width / $Text.Length
            $column = [int][Math]::Floor(($label[0].Left - $line[0].Left) / $cell)
            Send-AgentMouseClick -App $script:cmApp -PaneSessionId $script:cmPane -Column $column -Row $hits[0] | Out-Null
        }

        function Set-CmWindowWidth {
            param([int]$Width, [double]$Scale = 1)
            Set-WtWindowForeground -App $script:cmApp | Should -BeTrue
            $window = [IntPtr][long]$script:cmApp.Hwnd
            $process = Get-Process -Id $script:cmApp.Pid -ErrorAction Stop
            if ($script:cmApp.OwnedProcess.HasExited -or
                $process.StartTime -ne $script:cmApp.OwnedProcess.StartTime -or
                [ItE2E.ItWtWin32Input]::GetWindowProcessId($window) -ne $process.Id) {
                throw 'Resize lost the original test-owned window lease.'
            }
            $dpi = [ItE2E.ItWtWin32Input]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
            if ($dpi -eq [IntPtr]::Zero) { throw 'Physical window resize context is unavailable.' }
            try {
                $rect = [ItE2E.ItWtWin32Input+RECT]::new()
                [ItE2E.ItWtWin32Input]::GetWindowRect($window, [ref]$rect) | Should -BeTrue
                $originalWidth = $rect.Right - $rect.Left
                if (-not $Width) { $Width = [int]($originalWidth * $Scale) }
                [ItE2E.ItWtWin32Input]::SetWindowPos($window, [IntPtr]::Zero, 0, 0, $Width,
                    ($rect.Bottom - $rect.Top), 0x0016) | Should -BeTrue
                $originalWidth
            }
            finally { [void][ItE2E.ItWtWin32Input]::SetThreadDpiAwarenessContext($dpi) }
        }

        function Invoke-CmWheel {
            param([int]$Delta, [int]$Count, [string[]]$Present, [string[]]$Absent = @())
            $state = @{ Steps = 0 }
            Wait-Until -TimeoutSec 20 -IntervalSec 0.1 -Because 'native wheel navigation to the requested visible history' -Condition {
                $text = Read-CmPane
                $ready = $true
                foreach ($value in $Present) {
                    if (-not $text.Contains($value, [StringComparison]::Ordinal)) { $ready = $false }
                }
                foreach ($value in $Absent) {
                    if ($text.Contains($value, [StringComparison]::Ordinal)) { $ready = $false }
                }
                if ($ready) { return $text }
                if ($state.Steps -ge $Count) { throw 'Native wheel navigation exhausted its bounded input count.' }
                $anchor = [regex]::Match($text,
                    'CM_[0-9a-f]{12}_(?:ROW_\d{3}|PREFIX|TAIL|WRAP_START|WRAP_END|CELL)').Value
                $anchor | Should -Not -BeNullOrEmpty
                $bounds = @(Get-UiTextBounds -App $script:cmApp -Text $anchor)
                $bounds | Should -HaveCount 1
                Invoke-WtWindowWheel -App $script:cmApp -ScreenX ([int]($bounds[0].Left + $bounds[0].Width / 2)) `
                    -ScreenY ([int]($bounds[0].Top + $bounds[0].Height / 2)) -Delta $Delta -Count 1 | Out-Null
                $state.Steps++
                $false
            } | Out-Null
        }
    }

    BeforeEach {
        $script:cmApp = $null
        $script:cmPane = $null
        $script:cmClipboardSaved = $false
        $script:cmSavedLogFilter = $env:WTA_LOG
        $script:cmCaseNumber++
        $script:cmEvidence = Join-Path $script:cmRoot ("case-{0:D2}" -f $script:cmCaseNumber)
        $script:cmGates = Join-Path $script:cmEvidence 'gates'
        New-Item -ItemType Directory -Path $script:cmGates -Force | Out-Null
        $script:cmFixtureLog = Join-Path $script:cmEvidence 'fixture.log'
        $target = Resolve-ItApp -Package Dev
        Assert-WtPackageInactive -App $target
        foreach ($path in @($target.SettingsPath, $target.StatePath)) {
            if ((Test-Path "$path.e2ebak") -or (Test-Path "$path.e2ebak.missing")) { throw "Recover the existing backup first: $path" }
        }
        (Get-FileHash -LiteralPath (Join-Path $target.InstallLocation 'TerminalApp.dll')).Hash |
            Should -Be $env:ITE2E_EXPECTED_APP_SHA256
        (Get-FileHash -LiteralPath $target.WtaPath).Hash | Should -Be $env:ITE2E_EXPECTED_WTA_SHA256
        $script:cmClipboard = Get-ClipboardSnapshot
        $script:cmClipboardSaved = $true
        $fixture = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures\Mock-AcpInteractionAgent.ps1')).Path
        $invocation = "& '$($fixture.Replace("'", "''"))' -LogPath '$($script:cmFixtureLog.Replace("'", "''"))'" +
            " -CompletedMarkdownGateDirectory '$($script:cmGates.Replace("'", "''"))'"
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($invocation))
        $pwsh = (Get-Command pwsh -ErrorAction Stop).Source
        $profile = '{' + [guid]::NewGuid().ToString() + '}'
        $env:WTA_LOG = 'info,completed_markdown=debug'
        $script:cmApp = Start-Terminal -Package Dev -Backup $true -PassFre $true -State @{
            sidebarLayoutMigrationCompleted = $true; sidebarIntroductionShown = $true
        } -Settings @{
            language = 'en-US'; tabLayout = 'horizontal'; startupActions = ''
            initialCols = 160; initialRows = 64; launchMode = 'default'
            firstWindowPreference = 'defaultProfile'; windowingBehavior = 'useNew'
            acpAgent = 'custom:completed-markdown-fixture'; acpModel = ''
            acpCustomCommand = "pwsh -NoProfile -EncodedCommand $encoded"
            agentPanePosition = 'bottom'; autoErrorDetectionEnabled = $true; autoFixEnabled = $false
            copyOnSelect = $false; rightClickContextMenu = $false; 'warning.confirmOnClose' = 'never'
            defaultProfile = $profile
            profiles = @{ list = @(@{
                guid = $profile; name = 'Completed Markdown fixture'; commandline = "`"$pwsh`" -NoLogo -NoProfile -NoExit"
                startingDirectory = $script:cmEvidence; shellIntegrationEnabled = $true; font = @{ size = 10 }
                environment = @{
                    PATH = "$($target.InstallLocation);$env:PATH"
                    WT_WTCLI_PATH = $target.WtcliPath
                    WTCLI_PATH = $target.WtcliPath
                }
            }) }
            actions = @(@{ command = @{ action = 'resizePane'; direction = 'up' }; keys = 'ctrl+shift+f12' })
        }
        $script:cmApp | Add-Member -NotePropertyName RequireOwnedForeground -NotePropertyValue $true -Force
        Open-AgentPane -App $script:cmApp | Out-Null
        $script:cmSession = Wait-NewAgentPaneSession -App $script:cmApp -TimeoutSec 30
        @(Get-AgentPaneSessions -App $script:cmApp) | Should -HaveCount 1 -Because 'this creation-owned fixture window has one new agent pane'
        $script:cmPane = $script:cmSession.PaneSessionId
        Wait-AgentReady -App $script:cmApp -PaneSessionId $script:cmPane -TimeoutSec 30 | Should -BeTrue
        [string]::Equals((Get-Process -Id $script:cmSession.HelperProcessId).Path,
            $target.WtaPath, [StringComparison]::OrdinalIgnoreCase) | Should -BeTrue
        $loadedApp = @((Get-Process -Id $script:cmApp.Pid).Modules | Where-Object ModuleName -eq 'TerminalApp.dll')
        $loadedApp | Should -HaveCount 1
        [string]::Equals($loadedApp[0].FileName, (Join-Path $target.InstallLocation 'TerminalApp.dll'),
            [StringComparison]::OrdinalIgnoreCase) | Should -BeTrue
        Set-AgentPaneFocus -App $script:cmApp | Out-Null
        Set-WtWindowForeground -App $script:cmApp | Should -BeTrue -Because 'physical actions require the owned interactive window before any test action'
        for ($attempt = 0; $attempt -lt 8 -and (Get-CmGeometry).Rows -lt 26; $attempt++) {
            Send-WtWindowKey -App $script:cmApp -Vk 0x7B -Ctrl -Shift -RequireForeground | Out-Null
        }
        $geometry = Get-CmGeometry
        $geometry.Rows | Should -BeGreaterOrEqual 26 -Because 'grow only this test pane if the requested startup size was clamped'
        $geometry.Columns | Should -BeGreaterOrEqual 80
        $script:cmIdentity = Get-CmIdentity
        @{
            package = $target.Package; version = $target.Version; install = $target.InstallLocation
            app_sha256 = $env:ITE2E_EXPECTED_APP_SHA256; wta_sha256 = $env:ITE2E_EXPECTED_WTA_SHA256
            identity = ($script:cmIdentity | ConvertFrom-Json); rows = $geometry.Rows; columns = $geometry.Columns
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $script:cmEvidence 'identity.json') -Encoding utf8NoBOM
        Initialize-LogOffsets -App $script:cmApp | Out-Null
    }

    AfterEach {
        try {
            if ($script:cmGates -and (Test-Path -LiteralPath $script:cmGates)) {
                Set-Content -LiteralPath (Join-Path $script:cmGates 'abort') -Value 'release' -Encoding utf8NoBOM
            }
            if ($script:cmApp -and $script:cmPane) { Save-CmEvidence -Name teardown }
        }
        finally {
            try {
                if ($script:cmApp) { Stop-Terminal -App $script:cmApp }
            }
            finally {
                try {
                    if ($script:cmClipboardSaved) { Restore-ClipboardSnapshot -Snapshot $script:cmClipboard }
                }
                finally { $env:WTA_LOG = $script:cmSavedLogFilter }
            }
        }
    }

    It 'Completed replies render Markdown after raw streaming' {
        $turn = Start-CmTurn
        Wait-CmPane -Present "**$($turn.Prefix)**" -Absent $turn.Tail | Out-Null
        Assert-CmWorkStable -Count 0
        Save-CmEvidence -Name prefix-held
        Release-CmGate -Turn $turn -Gate tail
        Wait-CmPane -Present "**$($turn.Prefix)** tail **$($turn.Tail)**" | Out-Null
        Assert-CmWorkStable -Count 0
        Save-CmEvidence -Name all-source-finish-held
        Release-CmGate -Turn $turn -Gate finish
        Wait-CmFixture -Turn $turn -Event finished
        Wait-CmPane -Present "$($turn.Prefix) tail $($turn.Tail)" -Absent @("**$($turn.Prefix)**", "**$($turn.Tail)**") -Completed | Out-Null
        Wait-CmWork -Count 1
        Save-CmEvidence -Name completed
        Assert-CmIdentity
    }

    It 'Interrupted replies stay raw and preserve transcript order' {
        $turn = Start-CmTurn -Mode ORDER
        $command = "**CM_$($turn.Id)_COMMAND**"
        $output = "**CM_$($turn.Id)_OUTPUT**"
        $thought = "**CM_$($turn.Id)_THOUGHT**"
        Wait-CmPane -Present @("**$($turn.Prefix)**", $command, $output, "**$($turn.After)**", $turn.User) | Out-Null
        $thinkPattern = Get-WtaLocalizedTextRegex -Key 'chat.tool_kind.think'
        $thinkPattern | Should -Not -BeNullOrEmpty
        $thinkLabel = [regex]::Match((Read-CmPane), $thinkPattern).Value
        Invoke-CmTranscriptHeader -Text $thinkLabel
        Wait-CmPane -Present $thought | Out-Null
        Release-CmGate -Turn $turn -Gate tail
        $active = Wait-CmPane -Present @("**$($turn.Prefix)**", "**$($turn.After)** tail **$($turn.Tail)**", $thought)
        Assert-CmWorkStable -Count 0
        Save-CmEvidence -Name interleaved-held
        Release-CmGate -Turn $turn -Gate finish
        Wait-CmFixture -Turn $turn -Event finished
        $completed = Wait-CmPane -Present @($turn.Prefix, $turn.After, $turn.Tail, $turn.User, $command, $thought) `
            -Absent @("**$($turn.Prefix)**", "**$($turn.After)**", "**$($turn.Tail)**") -Completed
        if (-not $completed.Contains($output, [StringComparison]::Ordinal)) {
            Invoke-CmTranscriptHeader -Text $command
            $completed = Wait-CmPane -Present @($output, $thought, $turn.Tail) -Completed
        }
        foreach ($text in @($active, $completed)) {
            $previous = -1
            foreach ($value in @($turn.Prefix, $command, $output, $thought, $turn.After, $turn.Tail)) {
                $position = $text.IndexOf($value, [StringComparison]::Ordinal)
                $position | Should -BeGreaterThan $previous -Because 'agent, tool output, thought and trailing agent text retain arrival order'
                $previous = $position
            }
        }
        Wait-CmWork -Count 2
        Save-CmEvidence -Name interleaved-completed

        foreach ($mode in 'CANCEL', 'ERROR') {
            $partial = Start-CmTurn -Mode $mode
            Release-CmGate -Turn $partial -Gate tail
            Wait-CmPane -Present "**$($partial.Prefix)** tail **$($partial.Tail)**" | Out-Null
            Assert-CmWorkStable -Count 2
            if ($mode -eq 'CANCEL') {
                Set-AgentPaneFocus -App $script:cmApp | Out-Null
                Send-WtWindowKey -App $script:cmApp -Vk 0x43 -Ctrl -RequireForeground | Out-Null
                Wait-CmFixture -Turn $partial -Event cancelled
                $canceled = Get-WtaLocalizedTextRegex -Key 'chat.turn_canceled'
                $canceled | Should -Not -BeNullOrEmpty
                (Wait-CmPane -Present @("**$($partial.Prefix)**", "**$($partial.Tail)**") -Completed) | Should -Match $canceled
            }
            else {
                Release-CmGate -Turn $partial -Gate finish
                Wait-CmFixture -Turn $partial -Event error
                Wait-CmPane -Present @("**$($partial.Prefix)**", "**$($partial.Tail)**", "**CM_$($partial.Id)_ERROR**") -Completed | Out-Null
            }
            Assert-CmWorkStable -Count 2
            Save-CmEvidence -Name "interrupted-$mode"
        }
        Assert-CmIdentity
    }

    It 'Completed Markdown keeps cached history and selection geometry' {
        $geometry = Get-CmGeometry
        $turn = Start-CmTurn -Mode HISTORY -Rows ([Math]::Min(160, $geometry.Rows * 2))
        Release-CmGate -Turn $turn -Gate tail
        Wait-CmPane -Present "**$($turn.Tail)**" | Out-Null
        Assert-CmWorkStable -Count 0
        Release-CmGate -Turn $turn -Gate finish
        Wait-CmFixture -Turn $turn -Event finished
        Wait-CmPane -Present @($turn.Cell, $turn.Tail, $turn.WrapStart, $turn.WrapEnd) `
            -Absent @("**$($turn.Tail)**", '| :--- | ---: |') -Completed | Out-Null
        Wait-CmWork -Count 1
        Send-AgentKey -App $script:cmApp -PaneSessionId $script:cmPane -Key Tab | Out-Null
        Send-AgentKey -App $script:cmApp -PaneSessionId $script:cmPane -Key Enter | Out-Null
        Wait-CmPane -Present $turn.User -Absent @($turn.Cell, $turn.Tail) -Completed | Out-Null
        Assert-CmWorkStable -Count 1
        Save-CmEvidence -Name history-collapsed
        Send-AgentKey -App $script:cmApp -PaneSessionId $script:cmPane -Key Enter | Out-Null
        Send-AgentKey -App $script:cmApp -PaneSessionId $script:cmPane -Key Escape | Out-Null
        if (-not (Read-CmPane).Contains($turn.Cell, [StringComparison]::Ordinal)) {
            Wait-CmPane -Present $turn.User | Out-Null
            Invoke-CmWheel -Delta -120 -Count ([Math]::Min(100, $geometry.Rows * 2)) `
                -Present @($turn.Cell, $turn.Tail, $turn.WrapStart, $turn.WrapEnd)
        }
        Wait-CmPane -Present @($turn.Cell, $turn.Tail, $turn.WrapStart, $turn.WrapEnd) -Completed | Out-Null
        Assert-CmWorkStable -Count 1
        $wide = Get-CmGeometry
        $originalWidth = $null
        try {
            $originalWidth = Set-CmWindowWidth -Scale 0.65
            Test-Until -TimeoutSec 10 -IntervalSec 0.2 -Condition { (Get-CmGeometry).Columns -lt $wide.Columns } | Should -BeTrue
            Invoke-CmWheel -Delta -120 -Count ([Math]::Min(100, $geometry.Rows * 2)) `
                -Present @($turn.Cell, $turn.Tail, $turn.WrapStart, $turn.WrapEnd) `
                -Absent @("**$($turn.Tail)**", '| :--- | ---: |')
            Wait-CmPane -Present @($turn.Cell, $turn.Tail, $turn.WrapStart, $turn.WrapEnd) `
                -Absent @("**$($turn.Tail)**", '| :--- | ---: |') -Completed | Out-Null
            $narrow = Get-CmGeometry
            $narrow.Columns | Should -BeGreaterOrEqual 48 -Because 'the small table must fit at both measured widths'
            $spanRows = foreach ($view in @($wide, $narrow)) {
                $tableRow = @($view.Lines | Where-Object { $_.Contains($turn.Cell, [StringComparison]::Ordinal) })
                $tableRow | Should -HaveCount 1
                $tableRow[0].Contains('42', [StringComparison]::Ordinal) |
                    Should -BeTrue -Because 'both table columns must fit on the same captured row at both widths'
                $start = -1; $end = -1
                for ($row = 0; $row -lt $view.Lines.Count; $row++) {
                    if ($view.Lines[$row].Contains($turn.WrapStart, [StringComparison]::Ordinal)) { $start = $row }
                    if ($view.Lines[$row].Contains($turn.WrapEnd, [StringComparison]::Ordinal)) { $end = $row }
                }
                $start | Should -BeGreaterOrEqual 0
                $end | Should -BeGreaterThan $start
                $end - $start + 1
            }
            $spanRows[1] | Should -BeGreaterThan $spanRows[0] -Because 'Ratatui must reflow the same completed source at the measured narrower width'
            Test-Until -TimeoutSec 10 -IntervalSec 0.2 -Condition { @(Get-CmWork).Count -gt 1 } | Should -BeTrue
            Assert-CmWorkStable -Count @(Get-CmWork).Count
            Save-CmEvidence -Name history-narrow
            Select-CmVisibleText -Text $turn.Cell
            Copy-CmSelection -Expected $turn.Cell
        }
        finally {
            if ($originalWidth) { Set-CmWindowWidth -Width $originalWidth | Out-Null }
        }
        Test-Until -TimeoutSec 10 -IntervalSec 0.2 -Condition { (Get-CmGeometry).Columns -eq $wide.Columns } | Should -BeTrue
        Invoke-CmWheel -Delta -120 -Count ([Math]::Min(100, $geometry.Rows * 2)) `
            -Present @($turn.Cell, $turn.Tail) -Absent "**$($turn.Tail)**"
        Wait-CmPane -Present @($turn.Cell, $turn.Tail) -Absent "**$($turn.Tail)**" -Completed | Out-Null
        $work = @(Get-CmWork).Count
        Assert-CmWorkStable -Count $work
        Select-CmVisibleText -Text $turn.Cell
        Copy-CmSelection -Expected $turn.Cell
        Invoke-CmWheel -Delta 120 -Count ([Math]::Min(100, $geometry.Rows * 2)) `
            -Present $turn.Prefix -Absent @($turn.Tail, "**$($turn.Prefix)**")
        Wait-CmPane -Present $turn.Prefix -Absent @($turn.Tail, "**$($turn.Prefix)**") | Out-Null
        Save-CmEvidence -Name history-manual-scroll
        Invoke-CmWheel -Delta -120 -Count ([Math]::Min(100, $geometry.Rows * 2)) -Present @($turn.Cell, $turn.Tail)
        Wait-CmPane -Present @($turn.Cell, $turn.Tail) -Completed | Out-Null
        Send-AgentKey -App $script:cmApp -PaneSessionId $script:cmPane -Key Left -Count 3 | Out-Null
        Assert-CmWorkStable -Count $work
        Save-CmEvidence -Name history-wide-restored
        Assert-CmIdentity
    }

    It 'Selecting streamed text defers completed Markdown' {
        $old = Start-CmTurn
        Release-CmGate -Turn $old -Gate tail
        Release-CmGate -Turn $old -Gate finish
        Wait-CmFixture -Turn $old -Event finished
        Wait-CmPane -Present "$($old.Prefix) tail $($old.Tail)" -Absent "**$($old.Prefix)**" -Completed | Out-Null
        Wait-CmWork -Count 1
        $turn = Start-CmTurn
        Release-CmGate -Turn $turn -Gate tail
        $raw = "**$($turn.Prefix)**"
        Wait-CmPane -Present @("$raw tail **$($turn.Tail)**", $old.Prefix) -Absent "**$($old.Prefix)**" | Out-Null
        Assert-CmWorkStable -Count 1
        Select-CmVisibleText -Text $raw
        Save-CmEvidence -Name streaming-selected
        Release-CmGate -Turn $turn -Gate finish
        Wait-CmFixture -Turn $turn -Event finished
        Wait-CmPane -Present @("$raw tail **$($turn.Tail)**", $old.Prefix) -Absent "**$($old.Prefix)**" -Completed | Out-Null
        Save-CmEvidence -Name completed-selection-pinned
        Copy-CmSelection -Expected $raw
        Wait-CmPane -Present @("$($turn.Prefix) tail $($turn.Tail)", $old.Prefix) `
            -Absent @($raw, "**$($turn.Tail)**", "**$($old.Prefix)**") -Completed | Out-Null
        Wait-CmWork -Count 2
        Save-CmEvidence -Name selection-copied-and-cleared
        Assert-CmIdentity
    }
}
