[CmdletBinding()]
param([Parameter(Mandatory)][string]$RunDirectory)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$receipt = Get-Content (Join-Path $RunDirectory 'receipt.json') -Raw | ConvertFrom-Json
$events = Get-Content (Join-Path $RunDirectory 'code-integrity-events.json') -Raw | ConvertFrom-Json
$messages = @($receipt.windows | ForEach-Object children) + @($events | ForEach-Object message)
$prefixes = @($messages | ForEach-Object {
    [regex]::Matches($_, '_MEI[0-9a-fA-F]+\\') | ForEach-Object Value
} | Sort-Object -Unique)
if (-not $prefixes.Count) { throw 'No creation-correlated extraction paths were recorded.' }
if ($receipt.survivingOwnedPids.Count) { throw 'Stop the owned reproduction processes before dismissing their dialogs.' }

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class ReproErrorDialogs
{
    public sealed class Window { public long hwnd; public string text; }
    delegate bool Callback(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, Callback callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern IntPtr GetDlgItem(IntPtr hwnd, int id);
    [DllImport("user32.dll", SetLastError=true)]
    static extern IntPtr SendMessageTimeoutW(IntPtr hwnd, uint message, UIntPtr parameter, IntPtr data, uint flags, uint timeout, out UIntPtr result);
    public static Window[] Find()
    {
        var windows = new List<Window>();
        EnumWindows((hwnd, _) => {
            var title = new StringBuilder(512);
            GetWindowTextW(hwnd, title, title.Capacity);
            if (title.ToString() != "agy_acp_server.exe - Bad Image") return true;
            var text = new StringBuilder();
            EnumChildWindows(hwnd, (child, __) => {
                var part = new StringBuilder(4096);
                GetWindowTextW(child, part, part.Capacity);
                text.AppendLine(part.ToString());
                return true;
            }, IntPtr.Zero);
            windows.Add(new Window { hwnd=hwnd.ToInt64(), text=text.ToString() });
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
    public static void Acknowledge(long hwnd)
    {
        // Windows hard-error dialogs label IDCANCEL (2) as "OK".
        IntPtr button = GetDlgItem(new IntPtr(hwnd), 2);
        var text = new StringBuilder(32);
        GetWindowTextW(button, text, text.Capacity);
        if (button == IntPtr.Zero || text.ToString() != "OK")
            throw new InvalidOperationException("The recorded hard-error dialog has no expected OK button");
        UIntPtr result;
        if (SendMessageTimeoutW(new IntPtr(hwnd), 0x0111, new UIntPtr(2), button, 2, 1000, out result) == IntPtr.Zero)
            throw new Win32Exception(Marshal.GetLastWin32Error());
    }
}
'@

$record = [ordered]@{ started=(Get-Date).ToString('o'); extractionPaths=$prefixes; acknowledged=@(); status='running' }
try {
    for ($pass = 0; $pass -lt 12; $pass++) {
        $matching = @([ReproErrorDialogs]::Find() | Where-Object {
            $text = $_.text
            $text.Contains('0xc0e90002') -and @($prefixes | Where-Object { $text.Contains($_) }).Count -gt 0
        })
        if (-not $matching.Count) {
            $record.status = 'passed'
            break
        }
        foreach ($window in $matching) {
            [ReproErrorDialogs]::Acknowledge($window.hwnd)
            $record.acknowledged += [ordered]@{ hwnd=$window.hwnd; at=(Get-Date).ToString('o') }
        }
        Start-Sleep -Milliseconds 300
    }
    if ($record.status -ne 'passed') { throw 'Reproduction error dialogs did not finish dismissing within the bound.' }
} catch {
    $record.status = 'failed'
    $record.error = $_.Exception.Message
    throw
} finally {
    $record.ended = (Get-Date).ToString('o')
    $record | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $RunDirectory 'dialog-cleanup.json')
}
$record | ConvertTo-Json -Depth 5
