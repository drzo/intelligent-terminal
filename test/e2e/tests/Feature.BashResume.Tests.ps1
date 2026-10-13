#Requires -Modules @{ ModuleName='Pester'; ModuleVersion='5.0.0' }
# A held local CLI fixture crosses Bash input -> OSC -> pane binding -> saved
# state without hooks, conversations, provider authentication, or model requests.

BeforeDiscovery {
    $script:GitBash = 'C:\Program Files\Git\bin\bash.exe'
    $script:Ready = (Test-Path -LiteralPath $script:GitBash) -and
        [bool](Get-AppxPackage | Where-Object Name -like '*IntelligentTerminal*') -and
        [bool](Get-Command winapp -ErrorAction SilentlyContinue)
}

Describe 'Feature: Bash resume binding before conversation' -Tag 'Feature' -Skip:(-not $script:Ready) {
    BeforeAll {
        Import-Module (Join-Path $PSScriptRoot '..\ItE2E\ItE2E.psd1') -Force
        $script:GitBash = 'C:\Program Files\Git\bin\bash.exe'
        $script:app = $null
        $package = Get-ItTestPackage
        $resolved = Resolve-ItApp -Package $package
        if ($package -eq 'Store' -or $resolved.Package -notlike 'IntelligentTerminal*') {
            throw 'The Bash resume regression requires a feature-branch Dev package.'
        }
        if (-not $env:ITE2E_EXPECTED_APP_SHA256) { throw 'Set ITE2E_EXPECTED_APP_SHA256 to this feature build App DLL hash.' }
        (Get-FileHash -LiteralPath (Join-Path $resolved.InstallLocation 'TerminalApp.dll')).Hash |
            Should -BeExactly $env:ITE2E_EXPECTED_APP_SHA256

        $root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
        $header = Get-Content -LiteralPath (Join-Path $root 'src\cascadia\inc\BashShellIntegration.h') -Raw
        $source = [regex]::Match($header, '(?s)R"BASH\((.*?)\)BASH"')
        if (-not $source.Success) { throw 'Cannot extract the generated integration for this feature revision.' }
        $script:integration = Join-Path $TestDrive 'integration.sh'
        [IO.File]::WriteAllText($script:integration, $source.Groups[1].Value.Replace("`r", ''))

        if (-not ('ItE2E.BashResumeWindow' -as [type])) {
            Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace ItE2E {
    public static class BashResumeWindow {
        private delegate bool Callback(IntPtr hwnd, IntPtr parameter);
        [DllImport("user32.dll")] private static extern bool EnumWindows(Callback callback, IntPtr parameter);
        [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
        [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
        [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int length);
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
        public static IntPtr Find(uint pid) {
            var result = IntPtr.Zero;
            EnumWindows((hwnd, _) => {
                GetWindowThreadProcessId(hwnd, out var owner);
                if (owner == pid && !IsWindowVisible(hwnd)) {
                    var text = new StringBuilder(256);
                    GetWindowText(hwnd, text, text.Capacity);
                    if (text.ToString().StartsWith("Windows Terminal", StringComparison.Ordinal)) {
                        result = hwnd;
                        return false;
                    }
                }
                return true;
            }, IntPtr.Zero);
            return result;
        }
    }
}
'@
        }

        function Find-BashResumeCommand($value, [string]$session) {
            if ($value -is [System.Collections.IDictionary]) {
                if ($value.commandline -and $value.commandline.Contains($session)) { $value.commandline }
                foreach ($child in $value.Values) { Find-BashResumeCommand $child $session }
            }
            elseif ($value -is [array]) {
                foreach ($child in $value) { Find-BashResumeCommand $child $session }
            }
        }

        function Test-BashResumePersistence([string]$distro) {
            $script:app = Start-Terminal -Package (Get-ItTestPackage) -PassFre $true -Settings @{
                firstWindowPreference = 'persistedLayoutAndContent'
                agentSessionManagementEnabled = $false
                autoErrorDetectionEnabled = $false
                autoFixEnabled = $false
                profiles = @{
                    defaults = @{}
                    list = @(@{
                        guid = '{bee4cc01-0900-4900-8900-000000000001}'
                        name = 'Bash resume fixture baseline'
                        commandline = 'powershell.exe -NoLogo -NoProfile'
                    })
                }
                defaultProfile = '{bee4cc01-0900-4900-8900-000000000001}'
            }
            $command = "`"$script:GitBash`" --noprofile --norc -i"
            $integration = $script:integration.Replace('\', '/')
            if ($distro) {
                $converted = Invoke-Native -FilePath 'wsl.exe' -Arguments @('-d', $distro, '--exec', 'wslpath', '-a', $script:integration) -TimeoutSec 30
                $converted.ExitCode | Should -Be 0
                $integration = $converted.StdOut.Trim()
                $command = "wsl.exe -d $distro --cd /tmp --exec bash --noprofile --norc -i"
            }
            $tab = New-WtTab -App $script:app -Command $command -Title 'resume-before-conversation'
            $pane = $tab.session_id
            $session = "bash-resume-$([guid]::NewGuid())"
            $listener = $null
            try {
                Assert-Pane -App $script:app -SessionId $pane -Match 'bash-[0-9.]+\$' -TimeoutSec 20
                $listener = Start-WtEventListener -App $script:app -WaitForReady
                $setup = "HISTFILE=/dev/null; HISTCONTROL=; HISTIGNORE=; PS1='RESUME_READY> '; copilot() { printf 'CLI_READY\n'; read -r __held; }; source '$integration'"
                Invoke-RunCommand -App $script:app -SessionId $pane -Command $setup -SettleSec 1 | Out-Null
                Assert-Pane -App $script:app -SessionId $pane -Match '(?m)^RESUME_READY>' -TimeoutSec 20

                Invoke-RunCommand -App $script:app -SessionId $pane -Command "printf 'copilot --resume=$session\n'" -SettleSec 1 | Out-Null
                Invoke-RunCommand -App $script:app -SessionId $pane -Command 'printf "NEGATIVE_COMPLETED\n"' -SettleSec 1 | Out-Null
                Assert-Pane -App $script:app -SessionId $pane -Match '(?m)^NEGATIVE_COMPLETED\s*$' -TimeoutSec 20
                @(Get-WtEvents -Listener $listener -Predicate {
                    $_.method -eq 'session_born_bound' -and $_.params.agent_session_id -eq $session
                }).Count | Should -Be 0

                Send-WtInput -App $script:app -SessionId $pane -Text "copilot --model gpt-5 --resume='$session'"
                Send-WtKeys -App $script:app -SessionId $pane -Keys @('Enter')
                $birth = Wait-WtEvent -Listener $listener -TimeoutSec 20 -Predicate {
                    $_.method -eq 'session_born_bound' -and $_.params.agent_session_id -eq $session
                }
                $birth | Should -Not -BeNullOrEmpty
                $birth.params.pane_id | Should -Be $pane
                $birth.params.agent | Should -BeExactly 'copilot'
                if ($distro) {
                    $birth.params.wsl_distro | Should -BeExactly $distro
                    $birth.params.cwd | Should -BeExactly '/tmp'
                }
                else { $birth.params.PSObject.Properties.Name | Should -Not -Contain 'wsl_distro' }
                Assert-Pane -App $script:app -SessionId $pane -Match '(?m)^CLI_READY\s*$' -TimeoutSec 20
                @(Get-WtEvents -Listener $listener -Predicate {
                    $_.method -eq 'session_born_bound' -and $_.params.agent_session_id -eq $session
                }).Count | Should -Be 1

                Send-WtInput -App $script:app -SessionId $pane -Text 'fixture-exit'
                Send-WtKeys -App $script:app -SessionId $pane -Keys @('Enter')
                Wait-WtEvent -Listener $listener -TimeoutSec 20 -Predicate {
                    $_.method -eq 'agent_event' -and $_.params.event -eq 'agent.session.end' -and
                    $_.params.agent_session_id -eq $session -and $_.params.pane_id -eq $pane
                } | Should -Not -BeNullOrEmpty
                Send-WtInput -App $script:app -SessionId $pane -Text "copilot --resume='$session'"
                Send-WtKeys -App $script:app -SessionId $pane -Keys @('Enter')
                Test-Until -TimeoutSec 20 -IntervalSec 0.25 -Condition {
                    @(Get-WtEvents -Listener $listener -Predicate {
                        $_.method -eq 'session_born_bound' -and $_.params.agent_session_id -eq $session
                    }).Count -eq 2
                } | Should -BeTrue -Because 'resuming again after completion must establish a fresh binding'

                Stop-WtEventListener -Listener $listener
                $listener = $null
                $rootProcess = $script:app.OwnedProcess
                $ownedWta = @(Get-DescendantWtaIds -RootPid $rootProcess.Id -AsProcess -RootStartTime $rootProcess.StartTime -RootProcess $rootProcess)
                foreach ($child in $ownedWta) {
                    if (-not $child.HasExited) { Stop-Process -InputObject $child -Force -ErrorAction Stop }
                }
                $emperor = [ItE2E.BashResumeWindow]::Find([uint32]$script:app.Pid)
                $emperor | Should -Not -Be ([IntPtr]::Zero)
                [ItE2E.BashResumeWindow]::SendMessage($emperor, 0x0011, [IntPtr]::Zero, [IntPtr]::Zero) |
                    Should -Not -Be ([IntPtr]::Zero)
                [void][ItE2E.BashResumeWindow]::SendMessage($emperor, 0x0016, [IntPtr]::new(1), [IntPtr]::new(1))
                Test-Until -TimeoutSec 20 -IntervalSec 0.25 -Condition { $rootProcess.HasExited } | Should -BeTrue
                $state = Get-Content -LiteralPath $script:app.StatePath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
                $commands = @(Find-BashResumeCommand $state $session)
                $commands.Count | Should -Be 1
                $expected = if ($distro) {
                    "wsl.exe -d $distro --cd `"/tmp`" --exec bash -lc `"exec copilot --resume $session`""
                } else { "cmd.exe /d /s /c `"copilot --resume $session`"" }
                $commands[0] | Should -BeExactly $expected

            }
            catch {
                if (-not $script:app.OwnedProcess.HasExited) {
                    $capture = Get-WtCapture -App $script:app -SessionId $pane -MaxLines 80
                    $artifact = if ($env:ITE2E_ARTIFACT_ROOT) { $env:ITE2E_ARTIFACT_ROOT } else { $TestDrive }
                    $capture | Set-Content -LiteralPath (Join-Path $artifact "bash-resume-$session-failure.txt") -Encoding utf8
                    Write-ItLog -Level ERROR -Message "Bash resume fixture pane:`n$capture"
                }
                throw
            }
            finally { if ($listener) { Stop-WtEventListener -Listener $listener } }
        }
    }

    AfterEach {
        if ($script:app) {
            Stop-Terminal -App $script:app
            $script:app = $null
        }
    }

    It 'Git Bash resumes persist before the first conversation' {
        Test-BashResumePersistence ''
    }

    It 'WSL Bash resumes preserve the distro and Linux working directory' {
        $distro = $env:ITE2E_BASH_RESUME_WSL_DISTRO
        if (-not $distro) {
            Set-ItResult -Skipped -Because 'Set ITE2E_BASH_RESUME_WSL_DISTRO to an installed Bash distro.'
            return
        }
        if ($distro -notmatch '^[A-Za-z0-9_.+-]+$') { throw 'Unsafe fixture distro name.' }
        Test-BashResumePersistence $distro
    }
}
