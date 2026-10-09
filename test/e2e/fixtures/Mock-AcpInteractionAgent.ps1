param(
    [Parameter(Mandatory)][string]$LogPath,
    [string]$ResolverFixturePath,
    [string]$TelemetryFixturePath,
    [string]$CompletedMarkdownGateDirectory
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$sessionCounter = 0
$currentMode = 'ask'
$sessionMcpServers = @{}
$resolverFixture = if ($ResolverFixturePath) {
    Get-Content -LiteralPath $ResolverFixturePath -Raw | ConvertFrom-Json
}

function Write-ResolverEvidence {
    param([Parameter(Mandatory)][hashtable]$Record)
    $Record.pid = $PID
    Add-Content -LiteralPath $resolverFixture.evidencePath -Encoding utf8 -Value ($Record | ConvertTo-Json -Depth 30 -Compress)
}

function Invoke-ResolverFixtureTurn {
    param([Parameter(Mandatory)][string]$SessionId, [Parameter(Mandatory)][string]$PromptText)

    $case = @($resolverFixture.cases | Where-Object {
        $PromptText.Contains([string]$_.marker)
    })
    if ($case.Count -ne 1) { throw "Expected one resolver fixture case, found $($case.Count)" }
    $case = $case[0]
    $contractMatch = [regex]::Match($PromptText, '(?ms)^### Command Resolver Invocation\r?\n.*?```json\r?\n(?<json>.*?)\r?\n```')
    if (-not $contractMatch.Success) { throw 'Autofix did not advertise Command Resolver Invocation' }
    $contract = $contractMatch.Groups['json'].Value | ConvertFrom-Json
    Write-ResolverEvidence @{
        event = 'prompt-received'; sessionId = $SessionId; marker = $case.marker
        prompt = $PromptText; contract = $contract
    }

    if ($case.query) {
        if ($contract.executable -ne 'wta.exe' -or $contract.arguments[0] -ne 'resolve-command' -or
            $contract.arguments[1] -ne '<name>') {
            throw 'Unexpected command resolver invocation contract'
        }
        $executable = (Get-Command $contract.executable -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = $executable
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.Environment['WTA_LOG'] = 'debug'
        $psi.Environment['PATH'] = "$($resolverFixture.commandDirectory);$($psi.Environment['PATH'])"
        foreach ($argument in $contract.arguments) {
            $psi.ArgumentList.Add($(if ($argument -eq '<name>') { [string]$case.token } else { [string]$argument }))
        }
        Write-ResolverEvidence @{
            event = 'query-started'; sessionId = $SessionId; marker = $case.marker
            executable = $executable; arguments = @($psi.ArgumentList)
        }
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $psi
        try {
            if (-not $process.Start()) { throw 'Resolver process did not start' }
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(20000)) {
                $process.Kill($true)
                $process.WaitForExit()
                throw 'Agent-initiated command lookup exceeded 20 seconds'
            }
            $output = $stdout.GetAwaiter().GetResult()
            $errorOutput = $stderr.GetAwaiter().GetResult()
            if ($process.ExitCode -ne 0) { throw "Resolver failed ($($process.ExitCode)): $errorOutput" }
            Write-ResolverEvidence @{
                event = 'query-result'; sessionId = $SessionId; marker = $case.marker
                result = ($output | ConvertFrom-Json); stderr = $errorOutput
            }
        }
        finally { $process.Dispose() }
    }
    Write-ResolverEvidence @{ event = 'turn-completed'; sessionId = $SessionId; marker = $case.marker }
    Send-TextUpdate -SessionId $SessionId -Text "RESOLVER_FIXTURE_DONE:$($case.marker)"
}

function Send-AcpMessage {
    param([Parameter(Mandatory)][hashtable]$Message)

    [Console]::Out.WriteLine(($Message | ConvertTo-Json -Depth 30 -Compress))
    [Console]::Out.Flush()
}

function Write-FixtureLog {
    param([Parameter(Mandatory)][string]$Message)

    Add-Content -LiteralPath $LogPath -Value "$PID|$Message" -Encoding utf8
}

function Get-SessionConfigOptions {
    @(
        @{
            id = 'mode'
            name = 'Mode'
            category = 'mode'
            type = 'select'
            currentValue = $currentMode
            options = @(
                @{ value = 'ask'; name = 'Ask'; description = 'Ask before editing' }
                @{ value = 'code'; name = 'Code'; description = 'Edit files directly' }
            )
        }
        @{
            id = 'reasoning'
            name = 'Reasoning'
            category = 'thought_level'
            type = 'select'
            currentValue = 'medium'
            options = @(
                @{ value = 'medium'; name = 'Medium' }
                @{ value = 'high'; name = 'High' }
            )
        }
        @{
            id = 'model'
            name = 'Model'
            category = 'model'
            type = 'select'
            currentValue = 'fixture-model'
            options = @(
                @{ value = 'fixture-model'; name = 'Fixture Model' }
            )
        }
    )
}

function Send-TextUpdate {
    param(
        [Parameter(Mandatory)][string]$SessionId,
        [Parameter(Mandatory)][string]$Text
    )

    Send-AcpMessage @{
        jsonrpc = '2.0'
        method = 'session/update'
        params = @{
            sessionId = $SessionId
            update = @{
                sessionUpdate = 'agent_message_chunk'
                content = @{ type = 'text'; text = $Text }
            }
        }
    }
}

function Invoke-UserInputTool {
    param(
        [Parameter(Mandatory)]$Server
    )

    $headers = @{ 'mcp-protocol-version' = '2025-06-18' }
    foreach ($header in @($Server.headers)) {
        if ($header.name -and $header.value) {
            $headers[[string]$header.name] = [string]$header.value
        }
    }
    $body = @{
        jsonrpc = '2.0'
        id = 1
        method = 'tools/call'
        params = @{
            name = 'request_user_input'
            arguments = @{
                question = 'Choose the deterministic answer'
                choices = @('Alpha', 'Beta', 'Gamma', 'Delta')
                allow_freeform = $true
            }
        }
    } | ConvertTo-Json -Depth 12 -Compress
    Invoke-RestMethod -Method Post -Uri $Server.url -Headers $headers -ContentType 'application/json' -Body $body
}

function Invoke-SessionMcpTool {
    param(
        [Parameter(Mandatory)]$Server,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][hashtable]$Arguments,
        [Parameter(Mandatory)][int]$Id
    )

    $headers = @{ 'mcp-protocol-version' = '2025-06-18' }
    foreach ($header in @($Server.headers)) {
        if ($header.name -and $header.value) {
            $headers[[string]$header.name] = [string]$header.value
        }
    }
    $body = @{
        jsonrpc = '2.0'
        id = $Id
        method = 'tools/call'
        params = @{
            name = $Name
            arguments = $Arguments
        }
    } | ConvertTo-Json -Depth 12 -Compress
    Invoke-RestMethod -Method Post -Uri $Server.url -Headers $headers -ContentType 'application/json' -Body $body
}

$markdownTurn = $null
if ($CompletedMarkdownGateDirectory) {
    if (-not (Test-Path -LiteralPath $CompletedMarkdownGateDirectory -PathType Container)) {
        throw 'The completed Markdown gate directory must already exist.'
    }
    $reader = [IO.StreamReader]::new([Console]::OpenStandardInput(), [Text.Encoding]::UTF8)
    $readTask = $reader.ReadLineAsync()
}
while ($true) {
    # Only the opt-in Markdown fixture needs to accept cancellation while a turn is held.
    if ($markdownTurn) {
        $abort = Test-Path -LiteralPath (Join-Path $CompletedMarkdownGateDirectory 'abort')
        if ($abort -or [DateTime]::UtcNow -ge $markdownTurn.Deadline) {
            Send-AcpMessage @{ jsonrpc = '2.0'; id = $markdownTurn.Id
                error = @{ code = -32603; message = 'CM_GATE_ABORTED' } }
            Write-FixtureLog -Message "markdown|$($markdownTurn.Marker)|aborted"
            $markdownTurn = $null
        }
        elseif (-not $markdownTurn.TailSent -and
            (Test-Path -LiteralPath (Join-Path $CompletedMarkdownGateDirectory "$($markdownTurn.Marker).tail"))) {
            Send-TextUpdate -SessionId $markdownTurn.SessionId -Text $markdownTurn.Tail
            $markdownTurn.TailSent = $true
            Write-FixtureLog -Message "markdown|$($markdownTurn.Marker)|tail"
        }
        elseif ($markdownTurn.TailSent -and
            (Test-Path -LiteralPath (Join-Path $CompletedMarkdownGateDirectory "$($markdownTurn.Marker).finish"))) {
            if ($markdownTurn.Mode -eq 'ERROR') {
                Send-AcpMessage @{ jsonrpc = '2.0'; id = $markdownTurn.Id
                    error = @{ code = -32603; message = "**CM_$($markdownTurn.Marker)_ERROR**" } }
                Write-FixtureLog -Message "markdown|$($markdownTurn.Marker)|error"
            }
            else {
                Send-AcpMessage @{ jsonrpc = '2.0'; id = $markdownTurn.Id; result = @{ stopReason = 'end_turn' } }
                Write-FixtureLog -Message "markdown|$($markdownTurn.Marker)|finished"
            }
            $markdownTurn = $null
        }
    }
    if ($CompletedMarkdownGateDirectory) {
        if (-not $readTask.Wait(50)) { continue }
        $line = $readTask.GetAwaiter().GetResult()
        if ($null -ne $line) { $readTask = $reader.ReadLineAsync() }
    }
    else {
        $line = [Console]::In.ReadLine()
    }
    if ($null -eq $line) { break }
    $request = $line | ConvertFrom-Json
    switch ($request.method) {
        'initialize' {
            Write-FixtureLog -Message 'initialize'
            Send-AcpMessage @{
                jsonrpc = '2.0'
                id = $request.id
                result = @{
                    protocolVersion = 1
                    agentCapabilities = @{
                        mcpCapabilities = @{ http = $true; sse = $false }
                        sessionCapabilities = @{ close = @{} }
                    }
                    agentInfo = @{
                        name = 'Interaction Fixture'
                        version = '1.0.0'
                    }
                }
            }
        }
        'session/new' {
            $sessionCounter++
            $sessionId = "interaction-$PID-$sessionCounter"
            $server = @($request.params.mcpServers) | Select-Object -First 1
            $sessionMcpServers[$sessionId] = $server
            Write-FixtureLog -Message "session/new|$sessionId|mcp_server=$([string]$server.name)"
            $cwdJson = if ($null -eq $request.params.cwd) {
                'null'
            }
            else {
                ConvertTo-Json -InputObject ([string]$request.params.cwd) -Compress
            }
            Write-FixtureLog -Message "session/new-cwd|$sessionId|$cwdJson"
            Send-AcpMessage @{
                jsonrpc = '2.0'
                id = $request.id
                result = @{
                    sessionId = $sessionId
                    configOptions = @(Get-SessionConfigOptions)
                }
            }
        }
        'session/close' {
            Write-FixtureLog -Message "session/close|$($request.params.sessionId)"
            Send-AcpMessage @{
                jsonrpc = '2.0'
                id = $request.id
                result = @{ configOptions = @(Get-SessionConfigOptions) }
            }
        }
        'session/set_config_option' {
            $currentMode = [string]$request.params.value
            Write-FixtureLog -Message "session/set_config_option|$($request.params.configId)|$currentMode"
            Send-AcpMessage @{
                jsonrpc = '2.0'
                method = 'session/update'
                params = @{
                    sessionId = [string]$request.params.sessionId
                    update = @{
                        sessionUpdate = 'config_option_update'
                        configOptions = @(Get-SessionConfigOptions)
                    }
                }
            }
            Send-AcpMessage @{
                jsonrpc = '2.0'
                id = $request.id
                result = @{}
            }
        }
        'session/prompt' {
            $sessionId = [string]$request.params.sessionId
            $promptText = (@($request.params.prompt) | ForEach-Object text) -join "`n"
            Write-FixtureLog -Message "session/prompt|$sessionId|$promptText"

            if ($CompletedMarkdownGateDirectory -and
                $promptText -cmatch '\bCM_(STREAM|ORDER|CANCEL|ERROR|HISTORY)_([0-9a-f]{12})\b') {
                if ($markdownTurn) { throw 'Only one held Markdown fixture turn is supported.' }
                $mode = $Matches[1]
                $marker = $Matches[2]
                $prefix = "**CM_${marker}_PREFIX**"
                $tail = " tail **CM_${marker}_TAIL**"
                if ($mode -eq 'HISTORY') {
                    if ($promptText -notmatch '\bROWS=(\d+)\b' -or [int]$Matches[1] -notin 12..160) {
                        throw 'Markdown history requires a bounded ROWS=12..160 value.'
                    }
                    $rows = [int]$Matches[1]
                    $prefix += "`n`n" + ((1..$rows | ForEach-Object {
                        # Markdown hard breaks keep history taller than the measured viewport.
                        "CM_${marker}_ROW_$('{0:D3}' -f $_) history row.  "
                    }) -join "`n")
                    $tail = "`n`nCM_${marker}_WRAP_START " + ('ordinary wrapped words ' * 8) +
                        "CM_${marker}_WRAP_END`n`n| **Key** | Value |`n| :--- | ---: |`n" +
                        "| CM_${marker}_CELL | 42 |`n`n**CM_${marker}_TAIL**"
                }
                Send-TextUpdate -SessionId $sessionId -Text $prefix
                if ($mode -eq 'ORDER') {
                    Send-AcpMessage @{ jsonrpc = '2.0'; method = 'session/update'; params = @{
                        sessionId = $sessionId; update = @{
                            sessionUpdate = 'tool_call'; toolCallId = "cm-tool-$marker"
                            title = "**CM_${marker}_TOOL**"; kind = 'execute'; status = 'in_progress'
                            rawInput = @{ command = "echo **CM_${marker}_COMMAND**" }
                            content = @(); locations = @()
                        }
                    } }
                    Send-AcpMessage @{ jsonrpc = '2.0'; method = 'session/update'; params = @{
                        sessionId = $sessionId; update = @{
                            sessionUpdate = 'tool_call_update'; toolCallId = "cm-tool-$marker"; status = 'failed'
                            rawOutput = @{ stdout = "**CM_${marker}_OUTPUT**"; stderr = "**CM_${marker}_STDERR**"; exitCode = 7 }
                        }
                    } }
                    Send-AcpMessage @{ jsonrpc = '2.0'; method = 'session/update'; params = @{
                        sessionId = $sessionId; update = @{
                            sessionUpdate = 'agent_thought_chunk'
                            content = @{ type = 'text'; text = "**CM_${marker}_THOUGHT**" }
                        }
                    } }
                    Send-TextUpdate -SessionId $sessionId -Text "**CM_${marker}_AFTER**"
                }
                $markdownTurn = @{
                    Id = $request.id; SessionId = $sessionId; Mode = $mode; Marker = $marker
                    Tail = $tail; TailSent = $false; Deadline = [DateTime]::UtcNow.AddSeconds(120)
                }
                Write-FixtureLog -Message "markdown|$marker|prefix"
                continue
            }

            $telemetryCase = if ($TelemetryFixturePath -and (Test-Path -LiteralPath $TelemetryFixturePath)) {
                Get-Content -LiteralPath $TelemetryFixturePath -Raw | ConvertFrom-Json
            }
            if ($telemetryCase -and $telemetryCase.mode -eq 'offer' -and
                $promptText.Contains([string]$telemetryCase.marker)) {
                try {
                    $server = $sessionMcpServers[$sessionId]
                    if (-not $server) { throw 'Telemetry Autofix session has no Session MCP server' }
                    $output = ([string]$telemetryCase.outputPath).Replace("'", "''")
                    $marker = [string]$telemetryCase.marker
                    if ($marker -notmatch '^TELEMETRY_FIX_[0-9a-f]{32}$') { throw 'Invalid telemetry fixture marker' }
                    Write-FixtureLog -Message "telemetry-proposal-start|$sessionId|$marker"
                    $response = Invoke-SessionMcpTool -Server $server -Name 'run_command_in_current_shell' -Id 990 -Arguments @{
                        summary = "$marker harmless file marker"
                        command = "`$m='$marker'; Set-Content -LiteralPath '$output' -Value `$m; Write-Output `$m"
                    }
                    Write-FixtureLog -Message "telemetry-proposal-result|$marker|$($response | ConvertTo-Json -Depth 20 -Compress)"
                    Send-AcpMessage @{ jsonrpc = '2.0'; id = $request.id; result = @{ stopReason = 'end_turn' } }
                }
                catch {
                    Write-FixtureLog -Message "telemetry-proposal-error|$($_.Exception.Message)"
                    Send-AcpMessage @{ jsonrpc = '2.0'; id = $request.id; error = @{ code = -32603; message = $_.Exception.Message } }
                }
            }
            elseif ($promptText -match '(?m)TELEMETRY_CHAT_[0-9a-f]{32}') {
                $marker = $Matches[0]
                Send-TextUpdate -SessionId $sessionId -Text "ACK:$marker"
                Send-AcpMessage @{ jsonrpc = '2.0'; id = $request.id; result = @{ stopReason = 'end_turn' } }
                Write-FixtureLog -Message "telemetry-chat-complete|$sessionId|$marker"
            }
            elseif ($resolverFixture) {
                try {
                    Invoke-ResolverFixtureTurn -SessionId $sessionId -PromptText $promptText
                    Send-AcpMessage @{
                        jsonrpc = '2.0'
                        id = $request.id
                        result = @{ stopReason = 'end_turn' }
                    }
                }
                catch {
                    Write-ResolverEvidence @{ event = 'fixture-error'; sessionId = $sessionId; message = $_.Exception.Message }
                    Send-AcpMessage @{
                        jsonrpc = '2.0'
                        id = $request.id
                        error = @{ code = -32603; message = $_.Exception.Message }
                    }
                }
            }
            elseif ($promptText -match 'TOOL_FLOW') {
                Send-TextUpdate -SessionId $sessionId -Text 'BEFORE_TOOL_MARKER'
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    method = 'session/update'
                    params = @{
                        sessionId = $sessionId
                        update = @{
                            sessionUpdate = 'tool_call'
                            toolCallId = 'ite2e-tool'
                            title = 'Run integration command'
                            kind = 'execute'
                            status = 'in_progress'
                            rawInput = @{
                                command = 'echo TOOL_DETAIL_MARKER'
                                cwd = 'C:\ite2e-work'
                            }
                            content = @()
                            locations = @()
                        }
                    }
                }
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    method = 'session/update'
                    params = @{
                        sessionId = $sessionId
                        update = @{
                            sessionUpdate = 'tool_call_update'
                            toolCallId = 'ite2e-tool'
                            status = 'completed'
                            rawOutput = @{
                                stdout = 'TOOL_OUTPUT_MARKER'
                                exitCode = 7
                            }
                        }
                    }
                }
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    method = 'session/update'
                    params = @{
                        sessionId = $sessionId
                        update = @{
                            sessionUpdate = 'plan'
                            entries = @(
                                @{ content = 'PLAN_MARKER'; priority = 'medium'; status = 'completed' }
                            )
                        }
                    }
                }
                Send-TextUpdate -SessionId $sessionId -Text 'AFTER_TOOL_MARKER'
                Start-Sleep -Seconds 10
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
                Write-FixtureLog -Message 'tool-flow-complete'
            }
            elseif ($promptText -match 'ASK_INPUT') {
                $server = $sessionMcpServers[$sessionId]
                if (-not $server) {
                    throw 'session/new did not provide a Session MCP server'
                }
                $response = Invoke-UserInputTool -Server $server
                $result = $response.result.structuredContent | ConvertTo-Json -Depth 20 -Compress
                Write-FixtureLog -Message "user-input-result|$result"
                Send-TextUpdate -SessionId $sessionId -Text "INPUT_RESULT:$result"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            elseif ($promptText -match 'TAB_DIRECTION_(?<marker>[A-F0-9]+)') {
                $server = $sessionMcpServers[$sessionId]
                if (-not $server) {
                    throw 'session/new did not provide a Session MCP server'
                }
                $marker = $Matches.marker
                $response = Invoke-SessionMcpTool -Server $server -Name 'create_workspace' -Id 2 -Arguments @{
                    summary = "Direction $marker"
                    command = "echo $marker"
                    placement = 'new_tab'
                    split_direction = 'auto'
                }
                $result = $response.result.structuredContent | ConvertTo-Json -Depth 20 -Compress
                Write-FixtureLog -Message "tab-direction-result|$result"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            elseif ($promptText -match 'EMPTY_WORKSPACE_(?<marker>[A-F0-9]+)') {
                $server = $sessionMcpServers[$sessionId]
                if (-not $server) {
                    throw 'session/new did not provide a Session MCP server'
                }
                $marker = $Matches.marker
                $response = Invoke-SessionMcpTool -Server $server -Name 'create_workspace' -Id 3 -Arguments @{
                    summary = "Empty workspace EMPTY_COMMAND_SENTINEL_$marker"
                    placement = 'new_tab'
                }
                $result = $response.result.structuredContent | ConvertTo-Json -Depth 20 -Compress
                Write-FixtureLog -Message "empty-workspace-result|$result"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            elseif ($promptText -match 'DELEGATE_WORKSPACE_(?<marker>[A-F0-9]+)') {
                $server = $sessionMcpServers[$sessionId]
                if (-not $server) {
                    throw 'session/new did not provide a Session MCP server'
                }
                $marker = $Matches.marker
                $response = Invoke-SessionMcpTool -Server $server -Name 'delegate_task_in_new_workspace' -Id 4 -Arguments @{
                    summary = "Delegate workspace $marker"
                    task = "DELEGATED_TASK_$marker"
                    placement = 'new_tab'
                }
                $result = $response.result.structuredContent | ConvertTo-Json -Depth 20 -Compress
                Write-FixtureLog -Message "delegate-workspace-result|$result"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            elseif ($promptText -match '(?m)^TELEMETRY_READY_REFRESH_[0-9a-f]{32}\s*$') {
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    method = 'session/update'
                    params = @{
                        sessionId = $sessionId
                        update = @{
                            sessionUpdate = 'config_option_update'
                            configOptions = @(Get-SessionConfigOptions)
                        }
                    }
                }
                Send-TextUpdate -SessionId $sessionId -Text 'TELEMETRY_READY_REFRESH_DONE'
                Write-FixtureLog -Message "telemetry-ready-complete|$($Matches[0].Trim())"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            elseif ($promptText -match '(?m)^LIFETIME_(?:[0-9a-f]{12}|[0-9a-f]{32})\s*$') {
                $marker = $Matches[0].Trim()
                Send-TextUpdate -SessionId $sessionId -Text "ACK:$marker"
                Write-FixtureLog -Message "lifetime-ack|$sessionId|$marker"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
            else {
                Send-TextUpdate -SessionId $sessionId -Text "ACK:$promptText"
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    result = @{ stopReason = 'end_turn' }
                }
            }
        }
        'session/cancel' {
            if ($markdownTurn -and $markdownTurn.SessionId -eq $request.params.sessionId) {
                Send-AcpMessage @{ jsonrpc = '2.0'; id = $markdownTurn.Id; result = @{ stopReason = 'cancelled' } }
                Write-FixtureLog -Message "markdown|$($markdownTurn.Marker)|cancelled"
                $markdownTurn = $null
            }
        }
        default {
            if ($null -ne $request.id) {
                Send-AcpMessage @{
                    jsonrpc = '2.0'
                    id = $request.id
                    error = @{ code = -32601; message = 'Method not found' }
                }
            }
        }
    }
}
