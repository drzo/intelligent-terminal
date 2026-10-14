#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }

Describe 'Bash resume suite independent shell discovery' -Tag 'Unit' {
    BeforeAll {
        Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
        $script:suite = (Resolve-Path (Join-Path $PSScriptRoot '..\tests\Feature.BashResume.Tests.ps1')).Path
        $script:pwsh = (Get-Process -Id $PID).Path
    }

    It 'keeps WSL discoverable when Git Bash availability is <Available>' -TestCases @(
        @{ Available = $false },
        @{ Available = $true }
    ) {
        $probe = @"
function Get-AppxPackage { [pscustomobject]@{ Name = 'IntelligentTerminalDev' } }
function winapp {}
function Test-Path {
    param([string]`$LiteralPath)
    if (`$LiteralPath -eq 'C:\Program Files\Git\bin\bash.exe') { return `$$($Available.ToString().ToLowerInvariant()) }
    Microsoft.PowerShell.Management\Test-Path -LiteralPath `$LiteralPath
}
`$cfg = New-PesterConfiguration
`$cfg.Run.Path = '$($script:suite.Replace("'", "''"))'
`$cfg.Run.PassThru = `$true
`$cfg.Run.SkipRun = `$true
`$cfg.Output.Verbosity = 'None'
`$result = Invoke-Pester -Configuration `$cfg
@{
    suiteSkipped = `$result.Containers[0].Blocks[0].Skip
    tests = @(`$result.Containers[0].Blocks[0].Tests | Select-Object Name, Skip)
    errors = @(`$result.Containers.ErrorRecord | Where-Object { `$_ })
} | ConvertTo-Json -Depth 5 -Compress
"@
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probe))
        $result = Invoke-Native -FilePath $script:pwsh -Arguments @('-NoProfile', '-NonInteractive', '-EncodedCommand', $encoded) -TimeoutSec 30
        $result.TimedOut | Should -BeFalse
        $result.ExitCode | Should -Be 0 -Because $result.StdErr
        $discovery = $result.StdOut | ConvertFrom-Json
        $discovery.errors.Count | Should -Be 0
        $discovery.suiteSkipped | Should -BeFalse
        $discovery.tests.Count | Should -Be 2
        $discovery.tests[0].Skip | Should -Be (-not $Available)
        $discovery.tests[1].Skip | Should -BeFalse
    }
}
