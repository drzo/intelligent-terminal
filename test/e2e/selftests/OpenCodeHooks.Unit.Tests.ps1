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
}

Describe 'Unit: OpenCode hook component behavior' -Tag 'Unit' {
    It 'executes both plugin entrypoints and lifecycle regressions' {
        $node = Get-Command node -ErrorAction Stop
        $result = Invoke-Native -FilePath $node.Source -Arguments @(
            '--test', '--test-reporter=tap', (Join-Path $PSScriptRoot 'OpenCodeHooks.test.mjs')
        ) -TimeoutSec 30
        $result.TimedOut | Should -BeFalse
        $result.ExitCode | Should -Be 0 -Because "$($result.StdOut)`n$($result.StdErr)"
        $result.StdOut | Should -Match '# pass 8'
        $result.StdOut | Should -Match '# skipped 0'
    }

    It 'skips the <Case> case when the optional native CLI is absent' -TestCases @(
        @{ Case = 'OpenCode copied hooks upgrade without force' },
        @{ Case = 'OpenCode CLI loads copied hooks' }
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
            { & $script:nativeCases['OpenCode CLI loads copied hooks'] } | Should -Throw '*Reached selected native executable*'
            Should -Invoke Get-Command -Times 0 -Exactly -ParameterFilter { $Name -eq 'opencode' }
            Should -Invoke Invoke-Native -Times 1 -Exactly -ParameterFilter { $FilePath -eq $override }
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }

    It 'fails rather than skips an invalid explicit native executable' {
        $previous = $env:ITE2E_OPENCODE_EXE
        try {
            $env:ITE2E_OPENCODE_EXE = Join-Path $TestDrive 'missing-native.exe'
            Mock Get-Command { throw 'An explicit invalid path must not fall back to PATH' } -ParameterFilter { $Name -eq 'opencode' }
            Mock Set-ItResult {}
            Mock Invoke-Native { throw 'Invalid executables must not run' }
            { & $script:nativeCases['OpenCode CLI loads copied hooks'] } | Should -Throw '*set ITE2E_OPENCODE_EXE*'
            Should -Invoke Set-ItResult -Times 0 -Exactly
            Should -Invoke Invoke-Native -Times 0 -Exactly
        }
        finally {
            if ($null -eq $previous) { Remove-Item Env:\ITE2E_OPENCODE_EXE -ErrorAction SilentlyContinue }
            else { $env:ITE2E_OPENCODE_EXE = $previous }
        }
    }
}
