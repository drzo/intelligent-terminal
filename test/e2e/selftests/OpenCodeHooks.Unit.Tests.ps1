#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }

BeforeAll {
    Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
    $suite = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot '..\tests\Feature.OpenCodeHookLifecycle.Tests.ps1'), [ref]$null, [ref]$null)
    $script:nativeCases = @{}
    foreach ($command in $suite.FindAll({
        param($node)
        $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'It'
    }, $true)) {
        $script:nativeCases[$command.CommandElements[1].Value] = $command.CommandElements[2].ScriptBlock.GetScriptBlock()
    }
    $self = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot 'OpenCodeHooks.Unit.Tests.ps1'), [ref]$null, [ref]$null)
    $componentCase = $self.Find({
        param($node)
        $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'It' -and
            $node.CommandElements[1].Value -eq 'executes both plugin entrypoints and lifecycle regressions'
    }, $true)
    $script:nativeCases['Node component wrapper'] = $componentCase.CommandElements[2].ScriptBlock.GetScriptBlock()
    $script:nativeLoaderCase = 'OpenCode CLI loads copied hooks and native V2 TUI binds a viewed session'
    $script:nativeCases[$script:nativeLoaderCase] | Should -Not -BeNullOrEmpty -Because 'the native loader regression must track its current E2E title'
}

Describe 'Unit: OpenCode hook component behavior' -Tag 'Unit' {
    It 'executes both plugin entrypoints and lifecycle regressions' {
        $node = Get-Command node -ErrorAction SilentlyContinue
        if (-not $node) {
            Set-ItResult -Skipped -Because 'Node.js is not installed for the JavaScript component regressions'
            return
        }
        $result = Invoke-Native -FilePath $node.Source -Arguments @(
            '--experimental-vm-modules', '--test', '--test-reporter=tap', (Join-Path $PSScriptRoot 'OpenCodeHooks.test.mjs')
        ) -TimeoutSec 30
        $result.TimedOut | Should -BeFalse
        $result.ExitCode | Should -Be 0 -Because "$($result.StdOut)`n$($result.StdErr)"
        $result.StdOut | Should -Match '(?m)^# pass [1-9][0-9]*\s*$'
        $result.StdOut | Should -Match '# skipped 0'
    }

    It 'skips only the Node-dependent <Case> when Node.js is absent' -TestCases @(
        @{ Case = 'Node component wrapper' },
        @{ Case = 'OpenCode hook entrypoints deliver lifecycle to WTA' },
        @{ Case = 'OpenCode hook suppression prevents ghost sessions' }
    ) {
        param($Case)
        $previous = $script:node
        try {
            $script:node = $null
            Mock Get-Command { $null } -ParameterFilter { $Name -eq 'node' }
            Mock Set-ItResult {}
            Mock Invoke-Native { throw 'Node-dependent execution must not start' }
            & $script:nativeCases[$Case]
            Should -Invoke Set-ItResult -Times 1 -Exactly -ParameterFilter { $Skipped -and $Because -match 'Node.js is not installed' }
            Should -Invoke Invoke-Native -Times 0 -Exactly
        }
        finally { $script:node = $previous }
    }

    It 'skips the <Case> case when the optional native CLI is absent' -TestCases @(
        @{ Case = 'OpenCode copied hooks upgrade without force' },
        @{ Case = 'OpenCode CLI loads copied hooks and native V2 TUI binds a viewed session' }
    ) {
        param($Case)
        $previous = $env:ITE2E_OPENCODE_EXE
        try {
            Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue
            Mock Get-Command { $null } -ParameterFilter { $Name -eq 'opencode' }
            Mock Set-ItResult {}
            Mock Invoke-Native { throw 'Native execution must not start without the prerequisite' }
            & $script:nativeCases[$Case]
            Should -Invoke Set-ItResult -Times 1 -Exactly -ParameterFilter { $Skipped -and $Because -match 'opencode CLI is not installed' }
            Should -Invoke Invoke-Native -Times 0 -Exactly
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }

    It 'uses an explicit native executable without requiring an OpenCode PATH command' {
        $previous = $env:ITE2E_OPENCODE_EXE
        $override = Join-Path $TestDrive 'opencode-override.exe'
        Set-Content -LiteralPath $override -Value ''
        try {
            $env:ITE2E_OPENCODE_EXE = $override
            Mock Get-Command { throw 'PATH discovery must not precede the override' } -ParameterFilter { $Name -eq 'opencode' }
            Mock Invoke-Native { throw 'Reached selected native executable' }
            { & $script:nativeCases[$script:nativeLoaderCase] } | Should -Throw '*Reached selected native executable*'
            Should -Invoke Get-Command -Times 0 -Exactly -ParameterFilter { $Name -eq 'opencode' }
            Should -Invoke Invoke-Native -Times 1 -Exactly -ParameterFilter { $FilePath -eq $override }
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }

    It 'copies the selected packaged native adapter without changing default OpenCode discovery' {
        $previousOverride = $env:ITE2E_OPENCODE_EXE
        $previousPath = $env:PATH
        $previousApp = $script:app
        $previousRoots = $script:nativeRoots
        $pathDirectory = Join-Path $TestDrive 'path-v1'
        New-Item -ItemType Directory -Path $pathDirectory | Out-Null
        Set-Content -LiteralPath (Join-Path $pathDirectory 'opencode.cmd') -Value '@echo 1.0.0'
        $override = (Get-Command pwsh).Source
        try {
            $env:PATH = "$pathDirectory$([IO.Path]::PathSeparator)$previousPath"
            $env:ITE2E_OPENCODE_EXE = $override
            $script:app = [pscustomobject]@{
                WtaPath = 'fixture-wta.exe'
                InstallLocation = $TestDrive
                ComClsid = 'fixture-clsid'
            }
            $script:nativeRoots = [Collections.Generic.List[string]]::new()
            $oldBundle = $script:bundlePath
            $script:bundlePath = Join-Path $TestDrive 'bundle'
            foreach ($relative in @('shared\bridge.js', 'plugin.json', 'v2\wt-agent-hooks.js', 'v2\server.js', 'v2\rpc.js', 'v2\tui.js', 'v2\package.json')) {
                $path = Join-Path $script:bundlePath $relative
                New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
                Set-Content -LiteralPath $path -Value '// copied payload'
            }
            Mock Invoke-Native { [pscustomobject]@{ ExitCode = 0; StdOut = '2.0.0'; StdErr = '' } }
            Mock Get-FileHash {
                $support = Join-Path $script:nativeRoots[0] 'config\opencode\plugins\wt-agent-hooks'
                Test-Path -LiteralPath (Join-Path $support 'wt-agent-hooks.js') | Should -BeTrue
                throw 'Reached copied V2 payload'
            }
            { & $script:nativeCases[$script:nativeLoaderCase] } | Should -Throw '*Reached copied V2 payload*'
            Should -Invoke Invoke-Native -Times 1 -Exactly -ParameterFilter { $FilePath -eq $override }
            Should -Invoke Invoke-Native -Times 0 -Exactly -ParameterFilter { $FilePath -eq 'fixture-wta.exe' }
        }
        finally {
            foreach ($root in $script:nativeRoots) {
                if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
            }
            $script:app = $previousApp
            $script:nativeRoots = $previousRoots
            $script:bundlePath = $oldBundle
            $env:PATH = $previousPath
            if ($null -eq $previousOverride) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previousOverride }
        }
    }

    It 'fails rather than skips an invalid explicit native executable' {
        $previous = $env:ITE2E_OPENCODE_EXE
        try {
            $env:ITE2E_OPENCODE_EXE = Join-Path $TestDrive 'missing-native.exe'
            Mock Get-Command { throw 'An explicit invalid path must not fall back to PATH' } -ParameterFilter { $Name -eq 'opencode' }
            Mock Set-ItResult {}
            Mock Invoke-Native { throw 'Invalid executables must not run' }
            { & $script:nativeCases[$script:nativeLoaderCase] } | Should -Throw '*set ITE2E_OPENCODE_EXE*'
            Should -Invoke Set-ItResult -Times 0 -Exactly
            Should -Invoke Invoke-Native -Times 0 -Exactly
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }

    It 'rejects invalid native versions before copying either adapter' {
        $previous = $env:ITE2E_OPENCODE_EXE
        $override = Join-Path $TestDrive 'opencode-version-probe.exe'
        Set-Content -LiteralPath $override -Value ''
        try {
            $env:ITE2E_OPENCODE_EXE = $override
            Mock Write-ItLog {}
            Mock Set-ItResult {}
            Mock Invoke-Native { @{ ExitCode = 0; StdOut = 'unknown'; StdErr = '' } }
            { & $script:nativeCases[$script:nativeLoaderCase] } | Should -Throw
            Should -Invoke Set-ItResult -Times 0 -Exactly
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }
}
