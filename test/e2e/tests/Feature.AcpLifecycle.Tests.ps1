#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }
# Real Settings -> WTA -> deterministic ACP process; no model requests.

BeforeDiscovery {
    Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
}

Describe 'Feature: ACP provider lifecycle' -Tag 'Feature', 'AcpLifecycle' {
    BeforeAll {
        $ErrorActionPreference = 'Stop'
        Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
        $script:package = Get-ItTestPackage
        if ($script:package -eq 'Store') {
            throw 'ACP lifecycle regression requires an explicitly selected Dev package.'
        }
        $script:target = Resolve-ItApp -Package $script:package
        [xml]$manifest = Get-Content -LiteralPath (Join-Path $script:target.InstallLocation 'AppxManifest.xml') -Raw
        $aliases = @($manifest.SelectNodes(
            "//*[local-name()='Extension' and @Category='windows.appExecutionAlias' and @Executable='wta.exe']//*[local-name()='ExecutionAlias']"))
        if ($aliases.Count -ne 1) { throw 'The selected package must have one WTA alias.' }
        $script:wtaAlias = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\$($script:target.Package)\$($aliases[0].GetAttribute('Alias'))"
        if (-not (Test-Path -LiteralPath $script:wtaAlias)) { throw 'The selected package WTA alias is missing.' }
        foreach ($entry in @(
            @{ Path = $script:target.WtaPath; Hash = $env:ITE2E_EXPECTED_WTA_SHA256 },
            @{ Path = (Join-Path $script:target.InstallLocation 'TerminalApp.dll'); Hash = $env:ITE2E_EXPECTED_APP_SHA256 }
        )) {
            if ($entry.Hash -notmatch '^[a-fA-F0-9]{64}$') { throw 'Exact baseline/fixed hashes are required.' }
            (Get-FileHash -LiteralPath $entry.Path).Hash | Should -Be $entry.Hash
        }
        Assert-WtPackageInactive -App $script:target
        Backup-WtConfig -App $script:target
        $script:ownsConfig = $true
        Set-WtState -App $script:target -Key sidebarIntroductionShown -Value $true | Out-Null
        Set-WtState -App $script:target -Key sidebarLayoutMigrationCompleted -Value $true | Out-Null
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        $artifactRoot = if ($env:ITE2E_ARTIFACT_ROOT) { $env:ITE2E_ARTIFACT_ROOT } else { Join-Path $PSScriptRoot '..\artifacts' }
        $script:evidence = Join-Path $artifactRoot ("acp-lifecycle-" + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:evidence -Force | Out-Null
        $script:fixture = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures\Mock-AcpChatAgent.ps1')).Path
        [xml]$resources = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\..\..\src\cascadia\TerminalSettingsEditor\Resources\en-US\Resources.resw') -Raw
        $script:expectedStartupError = $resources.SelectSingleNode("/root/data[@name='AIAgents_AcpStartupFailed.Text']/value").InnerText
        $script:expectedStartupError | Should -Not -BeNullOrEmpty
        $script:requests = Join-Path $script:evidence 'requests.log'
        $script:control = Join-Path $script:evidence 'startup.json'
        @{ mode = 'fail' } | ConvertTo-Json | Set-Content -LiteralPath $script:control
        $invocation = "& '$($script:fixture.Replace("'", "''"))' -LogPath '$($script:requests.Replace("'", "''"))' -StartupControlPath '$($script:control.Replace("'", "''"))'"
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($invocation))
        $command = "pwsh.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand $encoded"
        $script:environmentBefore = @{}
        foreach ($name in @('PATH', 'WT_WTCLI_PATH', 'WTCLI_PATH', 'WT_COM_CLSID')) {
            $script:environmentBefore[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        }
        $path = "$($script:target.InstallLocation);$env:PATH"
        $env:PATH = $path
        $env:WT_WTCLI_PATH = $script:target.WtcliPath
        $env:WTCLI_PATH = $script:target.WtcliPath
        if ($script:target.PrivateProtocolClsid) { $env:WT_COM_CLSID = $script:target.PrivateProtocolClsid }
        $profile = '{a7ac13b4-3907-45b1-b508-36ddad7e608a}'
        $script:app = Start-Terminal -Package $script:package -Backup $false -CleanSettings $false -PassFre $true -Settings @{
            acpAgent = 'custom:pwsh'
            acpCustomCommand = $command
            acpModel = ''
            delegateAgent = 'copilot'
            agentSessionManagementEnabled = $false
            autoErrorDetectionEnabled = $false
            autoFixEnabled = $false
            tabLayout = 'horizontal'
            language = 'en-US'
            defaultProfile = $profile
            profiles = @{
                defaults = @{
                    environment = @{
                        PATH = $path
                        WT_WTCLI_PATH = $script:target.WtcliPath
                        WTCLI_PATH = $script:target.WtcliPath
                    }
                }
                list = @(@{
                    guid = $profile
                    name = 'ACP lifecycle shell'
                    commandline = 'pwsh.exe -NoLogo -NoProfile'
                    startingDirectory = $script:evidence
                })
            }
        }
        Initialize-LogOffsets -App $script:app | Out-Null

        function script:Open-LifecycleSettings {
            $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$script:app.Hwnd)
            if (-not $root -or $root.Current.ProcessId -ne $script:app.Pid) {
                throw 'Settings launcher ownership changed.'
            }
            $button = $root.FindFirst(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.AndCondition]::new(
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::ClassNameProperty, 'Microsoft.UI.Xaml.Controls.SplitButton'),
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::IsOffscreenProperty, $false)))
            if (-not $button) {
                $root.FindAll(
                    [Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.Condition]::TrueCondition
                ) | ForEach-Object {
                    [pscustomobject]@{
                        Name = $_.Current.Name
                        Id = $_.Current.AutomationId
                        Class = $_.Current.ClassName
                        Type = $_.Current.ControlType.ProgrammaticName
                        Offscreen = $_.Current.IsOffscreen
                    }
                } | ConvertTo-Json -Depth 3 |
                    Set-Content -LiteralPath (Join-Path $script:evidence 'startup-ui.json')
                throw 'The normal new-tab menu is unavailable; scoped UI diagnostics were saved.'
            }
            $expand = $button.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)
            $expand.Expand()
            Invoke-UiElement -App $script:app -Selector Settings -TimeoutSec 10 | Out-Null
        }

        function script:New-LifecycleFixture {
            param([string]$Mode = 'fail', [switch]$SpawnChild)
            $directory = Join-Path $script:evidence ([guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path $directory -Force | Out-Null
            $log = Join-Path $directory 'requests.log'
            $control = Join-Path $directory 'startup.json'
            @{ mode = $Mode; spawnChild = [bool]$SpawnChild } | ConvertTo-Json | Set-Content -LiteralPath $control
            $invocation = "& '$($script:fixture.Replace("'", "''"))' -LogPath '$($log.Replace("'", "''"))' -StartupControlPath '$($control.Replace("'", "''"))'"
            $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($invocation))
            [pscustomobject]@{
                Log = $log
                Control = $control
                Command = "pwsh.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand $encoded"
            }
        }

        function script:Get-LifecycleStarts {
            param($Fixture)
            @(Get-Content -LiteralPath $Fixture.Log | Where-Object { $_ -match '^\d+\|start\|' }).Count
        }

        function script:Get-LifecycleSelection {
            $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$script:app.Hwnd)
            if (-not $root -or $root.Current.ProcessId -ne $script:app.Pid) {
                throw 'Settings window ownership changed.'
            }
            $selector = $root.FindFirst(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.PropertyCondition]::new(
                    [Windows.Automation.AutomationElement]::AutomationIdProperty, 'AcpAgent'))
            if (-not $selector) { throw 'The ACP selector is missing.' }
            if ($selector.Current.ControlType -ne [Windows.Automation.ControlType]::ComboBox) {
                $selector = $selector.FindFirst(
                    [Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::ControlTypeProperty,
                        [Windows.Automation.ControlType]::ComboBox))
                if (-not $selector) { throw 'The ACP settings group has no provider ComboBox.' }
            }
            $selection = $selector.GetCurrentPattern([Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()
            if ($selection.Count -ne 1) { throw 'The ACP selector has no unique selected item.' }
            $selectedName = $selection[0].Current.Name
            if (-not $selectedName) {
                $selectedName = @($selection[0].FindAll(
                    [Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::ControlTypeProperty,
                        [Windows.Automation.ControlType]::Text)
                ) | ForEach-Object { $_.Current.Name } | Where-Object { $_ }) -join ' '
            }
            [pscustomobject]@{
                Enabled = $selector.Current.IsEnabled -and $selection[0].Current.IsEnabled
                Name = $selectedName
            }
        }
    }

    AfterAll {
        try {
            if ($script:app) { Stop-Terminal -App $script:app -RestoreSettings $false }
            if ($script:ownsConfig) {
                Assert-WtPackageInactive -App $script:target
                Restore-WtConfig -App $script:target
                $script:ownsConfig = $false
            }
        }
        finally {
            if ($script:environmentBefore) {
                foreach ($name in $script:environmentBefore.Keys) {
                    [Environment]::SetEnvironmentVariable($name, $script:environmentBefore[$name], 'Process')
                }
            }
        }
    }

    It 'Failed ACP startup stays selectable and exposes an explicit retry' {
        Open-LifecycleSettings
        Invoke-SettingsNav -App $script:app -NavItem AIAgentsNavItem | Out-Null
        Wait-Until -TimeoutSec 90 -IntervalSec 0.5 -Because 'the real model probe to report its failed initialization' -Condition {
            (Get-ItLogText -App $script:app -Name 'wta-probe.log' -SinceStart) -match 'probe-models failed'
        } | Out-Null
        $ownedWindows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
            [Windows.Automation.TreeScope]::Children,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$script:app.Pid))
        @($ownedWindows | Where-Object { $_.Current.Name -eq 'Microsoft Visual C++ Runtime Library' }).Count |
            Should -Be 0 -Because 'an unrelated Settings assertion cannot count as the startup-error oracle'
        $selection = Get-LifecycleSelection
        $selection.Enabled | Should -BeTrue -Because 'runtime failure must not disable provider selection'
        $selection.Name | Should -Match 'pwsh'
        Save-UiScreenshot -App $script:app -Path (Join-Path $script:evidence 'startup-failed.png') | Out-Null
        Test-UiElementExists -App $script:app -Selector AcpSetupFailure -TimeoutSec 5 |
            Should -BeTrue -Because 'Settings must show startup failure rather than silently leaving an empty model list'
        Test-UiElementEnabled -App $script:app -Selector RetryAcpSetup |
            Should -BeTrue -Because 'the user must be able to explicitly retry the selected provider'
        $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$script:app.Hwnd)
        $failure = $root.FindFirst(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::AutomationIdProperty, 'AcpSetupFailure'))
        $messages = @($failure.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Text)
        ) | Where-Object { -not $_.Current.IsOffscreen } | ForEach-Object { $_.Current.Name })
        $messages | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:evidence 'startup-accessible-text.json')
        $messages | Should -Contain $script:expectedStartupError -Because 'the accessible primary error must remain fully localized, without appended protocol diagnostics'
        @($messages | Where-Object { $_ -match '\[[a-z_]+ / [a-z_]+; \d+/\d+;' }).Count |
            Should -Be 0 -Because 'raw startup diagnostics belong in logs, not the primary error announcement'

        @{ mode = 'ready' } | ConvertTo-Json | Set-Content -LiteralPath $script:control
        Invoke-UiElement -App $script:app -Selector RetryAcpSetup | Out-Null
        $modelPicker = Wait-Until -TimeoutSec 45 -IntervalSec 0.5 -Because 'explicit retry to expose a real model picker' -Condition {
            $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$script:app.Hwnd)
            $group = $root.FindFirst(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.AndCondition]::new(
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::NameProperty, 'Model'),
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::ControlTypeProperty,
                        [Windows.Automation.ControlType]::Group)))
            if ($group) {
                @($group.FindAll(
                    [Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.PropertyCondition]::new(
                        [Windows.Automation.AutomationElement]::ControlTypeProperty,
                        [Windows.Automation.ControlType]::ComboBox)
                ) | Where-Object { -not $_.Current.IsOffscreen }) | Select-Object -First 1
            }
        }
        $modelPicker.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
        Wait-Until -TimeoutSec 10 -IntervalSec 0.5 -Because 'the discovered model to appear as a selectable model option' -Condition {
            (Get-UiTree -App $script:app -Depth 20) -match 'Lifecycle Fixture Model'
        } | Out-Null
        $modelPicker.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
        (Get-LifecycleSelection).Name | Should -Match 'pwsh'
        Save-UiScreenshot -App $script:app -Path (Join-Path $script:evidence 'startup-recovered.png') | Out-Null
    }

    It 'ACP startup budgets survive separate probe requests' {
        $fixture = New-LifecycleFixture
        for ($i = 0; $i -lt 2; $i++) {
            $result = Invoke-Native -FilePath $script:wtaAlias -Arguments @(
                'probe-models', '--agent', $fixture.Command, '--agent-id', 'custom:lifecycle-budget'
            ) -TimeoutSec 120
            $result.ExitCode | Should -Not -Be 0
        }
        (Get-LifecycleStarts $fixture) | Should -Be 3 -Because 'separate CLI processes must share one failure ceiling'
        @{ mode = 'ready' } | ConvertTo-Json | Set-Content -LiteralPath $fixture.Control
        $blocked = Invoke-Native -FilePath $script:wtaAlias -Arguments @(
            'probe-models', '--agent', $fixture.Command, '--agent-id', 'custom:lifecycle-budget'
        ) -TimeoutSec 30
        $blocked.ExitCode | Should -Not -Be 0
        (Get-LifecycleStarts $fixture) | Should -Be 3 -Because 'another request must not silently reset the budget'
        $retried = Invoke-Native -FilePath $script:wtaAlias -Arguments @(
            'probe-models', '--agent', $fixture.Command, '--agent-id', 'custom:lifecycle-budget', '--retry-startup'
        ) -TimeoutSec 60
        $retried.ExitCode | Should -Be 0
        ($retried.StdOut | ConvertFrom-Json).available_models.id | Should -Contain 'lifecycle-fixture-model'
        (Get-LifecycleStarts $fixture) | Should -Be 4
    }

    It 'ACP verification initializes once without creating a conversation' {
        $fixture = New-LifecycleFixture -Mode ready
        $result = Invoke-Native -FilePath $script:wtaAlias -Arguments @(
            'verify-acp', '--agent', $fixture.Command, '--agent-id', 'custom:lifecycle-verification'
        ) -TimeoutSec 60
        $result.ExitCode | Should -Be 0
        ($result.StdOut | ConvertFrom-Json).startup_verified | Should -BeTrue
        (Get-LifecycleStarts $fixture) | Should -Be 1
        $methods = @(Get-Content -LiteralPath $fixture.Log | Where-Object { $_ -match '^\d+\|method\|' })
        $methods | Should -HaveCount 1
        $methods[0] | Should -Match '\|method\|initialize$'
    }

    It 'Failed ACP startup reclaims descendants before another attempt' {
        $fixture = New-LifecycleFixture -SpawnChild
        $program = (Get-Command pwsh.exe -ErrorAction Stop).Source
        $unrelated = [Diagnostics.Process]::new()
        $unrelated.StartInfo.FileName = $program
        $unrelated.StartInfo.UseShellExecute = $false
        $unrelated.StartInfo.CreateNoWindow = $true
        foreach ($argument in @('-NoProfile', '-NonInteractive', '-Command', 'Start-Sleep -Seconds 120')) {
            $unrelated.StartInfo.ArgumentList.Add($argument)
        }
        [void]$unrelated.Start()
        try {
            $result = Invoke-Native -FilePath $script:wtaAlias -Arguments @(
                'probe-models', '--agent', $fixture.Command, '--agent-id', 'custom:lifecycle-cleanup'
            ) -TimeoutSec 120
            $result.ExitCode | Should -Not -Be 0
            $survivors = 0
            foreach ($line in @(Get-Content -LiteralPath $fixture.Log | Where-Object { $_ -match '^\d+\|child\|' })) {
                $record = $line.Split('|', 5)
                $process = Get-Process -Id ([int]$record[2]) -ErrorAction SilentlyContinue
                if (-not $process) { continue }
                if ($process.StartTime.ToUniversalTime().Ticks -ne [long]$record[3] -or $process.Path -ne $record[4]) {
                    throw 'A recorded fixture PID was reused; refusing unrelated process cleanup.'
                }
                $survivors++
                Stop-Process -Id $process.Id -Force
                $process.WaitForExit(5000) | Should -BeTrue
            }
            $survivors | Should -Be 0 -Because 'cleanup must complete before a failed startup is reported'
            $unrelated.HasExited | Should -BeFalse -Because 'only the ACP-owned process tree may be stopped'
        }
        finally {
            if (-not $unrelated.HasExited) { $unrelated.Kill(); $unrelated.WaitForExit() }
            $unrelated.Dispose()
        }
    }
}
