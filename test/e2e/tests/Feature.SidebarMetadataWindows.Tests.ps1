#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }

Describe 'Feature: sidebar metadata belongs to its window' -Tag @('Feature', 'SidebarMetadata') {
    BeforeAll {
        Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        if ($env:ITE2E_PACKAGE -ne 'Dev' -or
            -not $env:ITE2E_EXPECTED_APP_SHA256 -or -not $env:ITE2E_EXPECTED_WTA_SHA256) {
            throw 'Select the exact-source Dev package and provide its App and WTA hashes.'
        }
        $script:target = Resolve-ItApp -Package Dev
        Stop-StaleItInstances -App $script:target
        (Get-FileHash (Join-Path $script:target.InstallLocation 'TerminalApp.dll')).Hash |
            Should -Be $env:ITE2E_EXPECTED_APP_SHA256
        (Get-FileHash $script:target.WtaPath).Hash | Should -Be $env:ITE2E_EXPECTED_WTA_SHA256
        $script:app = $null

        function Get-RenderedMetadata {
            param($Window)
            $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr][long]$Window.Hwnd)
            $condition = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::AutomationIdProperty, 'HeaderMetadataTextBlock')
            @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) | Where-Object {
                $_.Current.ProcessId -eq $Window.Pid -and -not $_.Current.IsOffscreen -and
                    $_.Current.BoundingRectangle.Width -gt 0
            } | ForEach-Object { $_.Current.Name })
        }

        function Set-OnlyMetadataField {
            param($Window, [string]$Field)
            Set-WtWindowForeground -App $Window | Should -BeTrue
            foreach ($selector in @('RichTabAgentStatusVisibleItem', 'RichTabWorkingDirectoryVisibleItem', $Field)) {
                Invoke-UiElement -App $Window -Selector FilterTabsButton | Out-Null
                Invoke-UiElement -App $Window -Selector $selector | Out-Null
                Send-WtWindowKey -App $Window -Vk 0x1B -RequireForeground | Out-Null
            }
        }
    }

    AfterAll {
        if ($script:app) { Stop-Terminal -App $script:app }
    }

    It 'keeps Repository in window one while window two displays Branch' {
        $repository = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
        $profile = '{' + [guid]::NewGuid() + '}'
        $script:app = Start-Terminal -Package Dev -PassFre $true -State @{
            sidebarLayoutMigrationCompleted = $true; sidebarIntroductionShown = $true
        } -Settings @{
            language = 'en-US'; tabLayout = 'vertical'; defaultProfile = $profile; startupActions = ''
            firstWindowPreference = 'defaultProfile'; windowingBehavior = 'useNew'
            profiles = @{ list = @(@{
                guid = $profile; name = 'Metadata window shell'
                commandline = 'pwsh -NoLogo -NoProfile'; startingDirectory = $repository
            }) }
        }
        $firstId = [string]$script:app.WindowId
        $previousHwnds = @(Get-WtWindowHwnds -App $script:app | ForEach-Object { [string]$_.hwnd })
        Set-OnlyMetadataField -Window $script:app -Field RichTabRepositoryVisibleItem
        $expectedRepository = [regex]::Escape((Split-Path $repository -Leaf))
        Wait-Until -TimeoutSec 20 -Because 'the first window renders its selected repository' -Condition {
            @(Get-RenderedMetadata -Window $script:app | Where-Object { $_ -match "Repository: $expectedRepository" }).Count -eq 1
        } | Out-Null

        Send-WtWindowKey -App $script:app -Vk 0x4E -Ctrl -Shift -RequireForeground | Out-Null
        $secondId = Wait-Until -TimeoutSec 20 -Because 'the app opens a second window' -Condition {
            Get-WtWindows -App $script:app | Where-Object { [string]$_.window_id -ne $firstId } |
                Select-Object -First 1 -ExpandProperty window_id
        }
        $secondHwnd = Wait-Until -TimeoutSec 15 -Because 'the second window uses the same process' -Condition {
            Get-WtWindowHwnds -App $script:app | Where-Object {
                $_.pid -eq $script:app.Pid -and [string]$_.hwnd -notin $previousHwnds
            } | Select-Object -First 1 -ExpandProperty hwnd
        }
        $second = $script:app.PSObject.Copy()
        $second.WindowId = [string]$secondId
        $second.Hwnd = $secondHwnd
        Set-OnlyMetadataField -Window $second -Field RichTabBranchVisibleItem
        $branch = (& git -C $repository branch --show-current).Trim()
        Wait-Until -TimeoutSec 20 -Because 'each window renders its own selection' -Condition {
            @(Get-RenderedMetadata -Window $script:app | Where-Object {
                $_ -match "Repository: $expectedRepository" -and $_ -notmatch 'Branch:'
            }).Count -eq 1 -and
            @(Get-RenderedMetadata -Window $second | Where-Object {
                $_ -match ([regex]::Escape("Branch: $branch")) -and $_ -notmatch 'Repository:'
            }).Count -eq 1
        } | Out-Null
        foreach ($window in @($script:app, $second)) {
            Invoke-UiElement -App $window -Selector FilterTabsButton | Out-Null
            (Get-UiElement -App $window -Selector RichTabRepositoryVisibleItem).toggleState |
                Should -Be $(if ($window.WindowId -eq $firstId) { 'on' } else { 'off' })
            (Get-UiElement -App $window -Selector RichTabBranchVisibleItem).toggleState |
                Should -Be $(if ($window.WindowId -eq $firstId) { 'off' } else { 'on' })
            Send-WtWindowKey -App $window -Vk 0x1B -RequireForeground | Out-Null
        }
    }
}
