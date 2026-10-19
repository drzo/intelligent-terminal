[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Direct', 'Master')][string]$Mode,
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$LocalStateRoot,
    [ValidateRange(1, 180)][int]$Seconds = 15,
    [switch]$DefaultErrorMode
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (Test-Path -LiteralPath $OutputDirectory) {
    throw 'Use a fresh output directory for each reproduction.'
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
New-Item -ItemType Directory -Path $LocalStateRoot -Force | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$env:LOCALAPPDATA = (Resolve-Path -LiteralPath $LocalStateRoot).Path
$env:WTA_LOG = 'info'
$env:WT_WTCLI_PATH = Join-Path $OutputDirectory 'no-terminal-control-client.exe'
$env:WT_COM_CLSID = '{' + [guid]::NewGuid().ToString('D') + '}'

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public sealed class BoundedAcpProcess : IDisposable
{
    [StructLayout(LayoutKind.Sequential)]
    struct SA { public int length; public IntPtr descriptor; public int inherit; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct SI
    {
        public int cb;
        public string reserved, desktop, title;
        public uint x, y, width, height, charsX, charsY, fill, flags;
        public ushort show, reservedBytes;
        public IntPtr reservedPointer, stdin, stdout, stderr;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct PI { public IntPtr process, thread; public uint pid, tid; }
    [StructLayout(LayoutKind.Sequential)]
    struct BasicLimits
    {
        public long processTime, jobTime;
        public uint flags;
        public UIntPtr minWorkingSet, maxWorkingSet;
        public uint activeProcesses;
        public UIntPtr affinity;
        public uint priority, scheduling;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct Limits
    {
        public BasicLimits basic;
        public ulong readOps, writeOps, otherOps, readBytes, writeBytes, otherBytes;
        public UIntPtr processMemory, jobMemory, peakProcessMemory, peakJobMemory;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int left, top, right, bottom; }
    public sealed class Window
    {
        public long hwnd;
        public uint pid;
        public bool ownedPid;
        public string title, className;
        public string[] children;
        public Rect bounds;
    }
    delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr CreateJobObjectW(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetInformationJobObject(IntPtr job, int kind, ref Limits limits, uint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool QueryInformationJobObject(IntPtr job, int kind, IntPtr data, uint size, IntPtr returned);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool CreatePipe(out IntPtr read, out IntPtr write, ref SA attributes, uint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateFileW(string path, uint access, uint share, ref SA attributes, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string application, StringBuilder command, IntPtr processAttributes, IntPtr threadAttributes, bool inherit, uint flags, IntPtr environment, string cwd, ref SI startup, out PI information);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool WriteFile(IntPtr file, byte[] data, uint size, out uint written, IntPtr overlapped);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool TerminateProcess(IntPtr process, uint exitCode);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool WaitNamedPipeW(string name, uint timeout);
    [DllImport("user32.dll")]
    static extern bool EnumWindows(EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")]
    static extern bool EnumChildWindows(IntPtr parent, EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")]
    static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")]
    static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern int GetWindowTextW(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern int GetClassNameW(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll")]
    static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);

    IntPtr job, process, input;
    public uint Pid { get; private set; }
    static void Check(bool result) { if (!result) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    static void Close(ref IntPtr handle) { if (handle != IntPtr.Zero && handle != new IntPtr(-1)) { CloseHandle(handle); handle = IntPtr.Zero; } }

    public BoundedAcpProcess(string executable, string arguments, string outputDirectory, bool defaultErrorMode)
    {
        IntPtr read = IntPtr.Zero, stdout = IntPtr.Zero, stderr = IntPtr.Zero, thread = IntPtr.Zero;
        try
        {
            job = CreateJobObjectW(IntPtr.Zero, null);
            Check(job != IntPtr.Zero);
            var limits = new Limits();
            limits.basic.flags = 0x2000 | 0x8;
            limits.basic.activeProcesses = 20;
            Check(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf<Limits>()));
            var sa = new SA { length = Marshal.SizeOf<SA>(), inherit = 1 };
            Check(CreatePipe(out read, out input, ref sa, 0));
            Check(SetHandleInformation(input, 1, 0));
            stdout = CreateFileW(System.IO.Path.Combine(outputDirectory, "stdout.txt"), 0x40000000, 7, ref sa, 1, 0x80, IntPtr.Zero);
            Check(stdout != new IntPtr(-1));
            stderr = CreateFileW(System.IO.Path.Combine(outputDirectory, "stderr.txt"), 0x40000000, 7, ref sa, 1, 0x80, IntPtr.Zero);
            Check(stderr != new IntPtr(-1));
            var si = new SI { cb = Marshal.SizeOf<SI>(), flags = 0x100, stdin = read, stdout = stdout, stderr = stderr };
            PI pi;
            uint creationFlags = 0x08000004;
            if (defaultErrorMode) creationFlags |= 0x04000000;
            Check(CreateProcessW(executable, new StringBuilder("\"" + executable + "\" " + arguments),
                IntPtr.Zero, IntPtr.Zero, true, creationFlags, IntPtr.Zero, outputDirectory, ref si, out pi));
            process = pi.process;
            thread = pi.thread;
            Pid = pi.pid;
            Check(AssignProcessToJobObject(job, process));
            Check(ResumeThread(thread) != UInt32.MaxValue);
        }
        catch
        {
            if (process != IntPtr.Zero) TerminateProcess(process, 1);
            Dispose();
            throw;
        }
        finally { Close(ref read); Close(ref stdout); Close(ref stderr); Close(ref thread); }
    }

    public void Send(string text)
    {
        byte[] data = Encoding.UTF8.GetBytes(text);
        uint written;
        Check(WriteFile(input, data, (uint)data.Length, out written, IntPtr.Zero));
        if (written != data.Length) throw new InvalidOperationException("Partial ACP input write");
    }

    public int[] ProcessIds()
    {
        int size = 8 + 128 * IntPtr.Size;
        IntPtr buffer = Marshal.AllocHGlobal(size);
        try
        {
            Check(QueryInformationJobObject(job, 3, buffer, (uint)size, IntPtr.Zero));
            int count = Marshal.ReadInt32(buffer, 4);
            if (count < 0 || count > 128) throw new InvalidOperationException("Invalid job process count");
            var ids = new int[count];
            for (int i = 0; i < count; i++) ids[i] = (int)Marshal.ReadIntPtr(buffer, 8 + i * IntPtr.Size);
            return ids;
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    public static Window[] Windows(int[] processIds)
    {
        var ids = new HashSet<int>(processIds);
        var found = new List<Window>();
        EnumWindows((hwnd, _) =>
        {
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            if (!IsWindowVisible(hwnd)) return true;
            var title = new StringBuilder(2048);
            var className = new StringBuilder(128);
            GetWindowTextW(hwnd, title, title.Capacity);
            bool owned = ids.Contains((int)pid);
            if (!owned && !title.ToString().StartsWith("agy_acp_server.exe -", StringComparison.Ordinal)) return true;
            GetClassNameW(hwnd, className, className.Capacity);
            var children = new List<string>();
            EnumChildWindows(hwnd, (child, __) =>
            {
                var text = new StringBuilder(4096);
                GetWindowTextW(child, text, text.Capacity);
                if (text.Length > 0) children.Add(text.ToString());
                return true;
            }, IntPtr.Zero);
            Rect bounds;
            GetWindowRect(hwnd, out bounds);
            found.Add(new Window { hwnd = hwnd.ToInt64(), pid = pid, ownedPid = owned, title = title.ToString(),
                className = className.ToString(), children = children.ToArray(), bounds = bounds });
            return true;
        }, IntPtr.Zero);
        return found.ToArray();
    }

    public void Dispose() { Close(ref job); Close(ref input); Close(ref process); }
}
'@

$pipe = '\\.\pipe\wta-antigravity-repro-' + [guid]::NewGuid().ToString('D')
$arguments = if ($Mode -eq 'Master') {
    '--master "' + $pipe + '" --agent agy_acp_server.exe --agent-id antigravity --allowed-agent-ids antigravity --no-session-management'
} else { '' }
$receipt = [ordered]@{
    phase = $Mode
    status = 'running'
    started = (Get-Date).ToString('o')
    executable = $Executable
    sha256 = (Get-FileHash -LiteralPath $Executable).Hash
    arguments = $arguments
    budgetSeconds = $Seconds
    hardJobProcessLimit = 20
    localAppData = $env:LOCALAPPDATA
    tempUnchanged = $env:TEMP
    isolatedControlClsid = $env:WT_COM_CLSID
    childDefaultErrorMode = [bool]$DefaultErrorMode
    pipeReady = $false
    maximumLiveProcesses = 0
    windows = @()
    captureErrors = @()
}
$receipt | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutputDirectory 'receipt.json')
$seen = @{}
$captured = @{}
$process = $null
$clock = [Diagnostics.Stopwatch]::StartNew()
try {
    $process = [BoundedAcpProcess]::new($Executable, $arguments, $OutputDirectory, [bool]$DefaultErrorMode)
    $receipt.rootPid = $process.Pid
    if ($Mode -eq 'Direct') {
        $initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"terminal":true},"clientInfo":{"name":"bounded-local-reproduction","version":"1"}}}'
        $initialize | Set-Content (Join-Path $OutputDirectory 'initialize.json')
        $process.Send($initialize + "`n")
    }
    while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
        $ids = @($process.ProcessIds())
        $rows = @()
        if ($ids.Count -gt 0) {
            $filter = ($ids | ForEach-Object { "ProcessId=$_" }) -join ' OR '
            $rows = @(Get-CimInstance Win32_Process -Filter $filter | Select-Object ProcessId, ParentProcessId, Name, CreationDate, ExecutablePath)
        }
        foreach ($row in $rows) {
            $key = "$($row.ProcessId):$($row.CreationDate.ToString('o'))"
            if (-not $seen.ContainsKey($key)) {
                $seen[$key] = [ordered]@{
                    pid = $row.ProcessId
                    parentPid = $row.ParentProcessId
                    name = $row.Name
                    created = $row.CreationDate.ToString('o')
                    path = $row.ExecutablePath
                    firstObservedSeconds = [math]::Round($clock.Elapsed.TotalSeconds, 3)
                }
            }
        }
        $windows = @([BoundedAcpProcess]::Windows([int[]]$ids))
        foreach ($window in $windows) {
            $key = "$($window.pid):$($window.hwnd)"
            if ($captured.ContainsKey($key)) { continue }
            $captured[$key] = $true
            $receipt.windows += $window
            $width = $window.bounds.right - $window.bounds.left
            $height = $window.bounds.bottom - $window.bounds.top
            if ($width -le 0 -or $height -le 0 -or $width -gt 5000 -or $height -gt 5000) {
                $receipt.captureErrors += "Invalid bounds for owned HWND $($window.hwnd)"
                continue
            }
            $bitmap = [Drawing.Bitmap]::new($width, $height)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $dc = $graphics.GetHdc()
            try {
                $ok = [BoundedAcpProcess]::PrintWindow([IntPtr]$window.hwnd, $dc, 2)
            } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
            try {
                if (-not $ok) { throw "PrintWindow failed for owned HWND $($window.hwnd)" }
                $bitmap.Save((Join-Path $OutputDirectory "window-$($window.pid)-$($window.hwnd).png"), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $bitmap.Dispose() }
        }
        $receipt.maximumLiveProcesses = [math]::Max($receipt.maximumLiveProcesses, $ids.Count)
        if ($Mode -eq 'Master' -and -not $receipt.pipeReady) {
            $receipt.pipeReady = [BoundedAcpProcess]::WaitNamedPipeW($pipe, 1)
        }
        [ordered]@{
            at = (Get-Date).ToString('o')
            elapsedSeconds = [math]::Round($clock.Elapsed.TotalSeconds, 3)
            processes = $rows
            windows = $windows
        } | ConvertTo-Json -Depth 6 -Compress | Add-Content (Join-Path $OutputDirectory 'timeline.jsonl')
        if ($ids.Count -ge 16) { $receipt.stopReason = 'Safety process threshold'; break }
        $attempts = @($seen.Values | Where-Object { $_.name -eq 'agy_acp_server.exe' -and $_.parentPid -eq $process.Pid })
        if ($Mode -eq 'Master' -and $attempts.Count -ge 6) { $receipt.stopReason = 'Six ACP attempts observed'; break }
        if ($Mode -eq 'Direct' -and $ids.Count -eq 0) { $receipt.stopReason = 'Provider exited'; break }
        Start-Sleep -Milliseconds 500
    }
    if (-not $receipt.Contains('stopReason')) { $receipt.stopReason = 'Bounded observation elapsed' }
    $receipt.aliveBeforeCleanup = @($process.ProcessIds())
    $receipt.status = 'observed'
} catch {
    $receipt.status = 'failed'
    $receipt.error = $_.Exception.Message
    throw
} finally {
    if ($null -ne $process) { $process.Dispose() }
    $receipt.processes = @($seen.Values | Sort-Object firstObservedSeconds)
    $survivors = @()
    foreach ($item in $receipt.processes) {
        $current = Get-CimInstance Win32_Process -Filter "ProcessId=$($item.pid)"
        if ($current -and $current.CreationDate.ToString('o') -eq $item.created) {
            $survivors += $item.pid
        }
    }
    $receipt.survivingOwnedPids = $survivors
    $receipt.ended = (Get-Date).ToString('o')
    $receipt.elapsedSeconds = [math]::Round($clock.Elapsed.TotalSeconds, 3)
    $receipt | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $OutputDirectory 'receipt.json')
}
$receipt | ConvertTo-Json -Depth 8
if ($receipt.survivingOwnedPids.Count -gt 0) { throw 'Owned processes survived job cleanup; inspect the receipt.' }
