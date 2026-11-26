<#
.SYNOPSIS
  Strategy Desk performance bench: cold / warm start, idle RAM and CPU,
  sizes, optional frame stats under a fake Claude stream and a soak run.
  Writes docs/rust/perf/<date>-<commit>.json (schema desk-perf/1, see
  docs/rust/PERF.md). Same script for the Rust build and the Electron 0.2.3
  baseline.

.DESCRIPTION
  Windows PowerShell 5.1 (the one every Windows 10/11 has). The file is
  ASCII apart from its UTF-8 BOM, because 5.1 reads a BOM-less script in the
  ANSI code page.

  Never launches, closes or talks to TradingView. Refuses to run between
  16:25 and 18:30 Kyiv time (the trading window) unless -Force, and stops a
  run that would reach 16:25.

.EXAMPLE
  # Electron 0.2.3 baseline (finds the installed app by itself):
  powershell -ExecutionPolicy Bypass -File scripts\rust\bench.ps1 -Kind electron

.EXAMPLE
  # Rust build from this checkout:
  powershell -ExecutionPolicy Bypass -File scripts\rust\bench.ps1 -Kind rust -Exe rust\target\release\strategy-desk.exe
#>
[CmdletBinding()]
param(
    [string]$Exe = '',
    [ValidateSet('rust', 'electron')][string]$Kind = 'rust',
    [int]$Runs = 10,
    [string]$Out = '',
    [int]$IdleRuns = 3,
    [int]$IdleSeconds = 60,
    [int]$SettleSeconds = 10,
    [ValidateSet('auto', 'admin', 'pressure', 'none')][string]$ColdMethod = 'auto',
    [string]$Installer = '',
    [string]$Commit = '',
    [string]$Label = '',
    [int]$StartTimeoutSeconds = 60,
    [string]$Vault = '',
    [string[]]$Variants = @(),
    [int]$VariantRuns = 0,
    [int]$SwitchCount = 1500,
    [switch]$SkipCold,
    [switch]$SkipWarm,
    [switch]$SkipIdle,
    [switch]$StreamFrames,
    [string]$FakeClaude = '',
    [int]$StreamEvents = 10000,
    [double]$StreamRate = 200,
    [double]$SoakHours = 0,
    [int]$SoakIntervalMinutes = 5,
    [switch]$Force,
    [switch]$SelfTest
)

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'
# PowerShell 5.1 serializes some arrays as {"value": [...], "Count": n}
# because of the ETS Count alias on System.Array; without it they stay arrays.
Remove-TypeData -TypeName System.Array -ErrorAction SilentlyContinue
$ScriptVersion = '2'
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# ---------------------------------------------------------------- catalog
# Mirror of rust/crates/desk-bench/src/report.rs METRICS (id, unit, target, stat).
# Russian labels live in the Rust catalog and report.py; this file stays ASCII.
$Catalog = @{
    'cold_start_ms'           = @('ms', 300, 'median', 'Cold start to first frame')
    'warm_start_ms'           = @('ms', 150, 'median', 'Warm start to first frame')
    'data_ready_cold_ms'      = @('ms', 500, 'median', 'Cold start to the day data drawn')
    'data_ready_warm_ms'      = @('ms', 300, 'median', 'Warm start to the day data drawn')
    'idle_working_set_mb'     = @('MB', 80, 'median', 'Idle RAM, Working Set')
    'idle_private_mb'         = @('MB', 80, 'median', 'Idle RAM, Private Bytes')
    'idle_cpu_pct'            = @('%', 1, 'median', 'Idle CPU')
    'idle_frames_per_min'     = @('1/min', $null, 'median', 'Frames per minute when idle')
    'frame_p50_ms'            = @('ms', 8, 'median', 'Frame p50 while streaming')
    'frame_p95_ms'            = @('ms', 16, 'max', 'Frame p95 while streaming')
    'frame_p99_ms'            = @('ms', 33, 'max', 'Frame p99 while streaming')
    'switch_frame_p50_ms'     = @('ms', 8, 'median', 'Frame p50 while switching days')
    'switch_frame_p95_ms'     = @('ms', 16, 'max', 'Frame p95 while switching days')
    'switch_frame_p99_ms'     = @('ms', 33, 'max', 'Frame p99 while switching days')
    'switch_ms'               = @('ms', 100, 'p95', 'Day switch, command to frame')
    'exe_mb'                  = @('MB', 15, 'max', 'exe size')
    'install_dir_mb'          = @('MB', $null, 'max', 'Installed folder size')
    'installer_mb'            = @('MB', 20, 'max', 'Installer size')
    'soak_private_growth_pct' = @('%', 5, 'max', 'Private Bytes growth over the soak')
}

# ---------------------------------------------------------------- stats
function Get-Median([double[]]$xs) {
    if ($null -eq $xs -or $xs.Count -eq 0) { return $null }
    $s = @($xs | Sort-Object)
    $n = $s.Count
    if ($n % 2 -eq 1) { return [double]$s[[int](($n - 1) / 2)] }
    return ([double]$s[$n / 2 - 1] + [double]$s[$n / 2]) / 2.0
}

# Nearest rank, no interpolation: with 10 samples p95 is the largest one.
function Get-Percentile([double[]]$xs, [double]$p) {
    if ($null -eq $xs -or $xs.Count -eq 0) { return $null }
    $s = @($xs | Sort-Object)
    $rank = [int][math]::Ceiling(($p / 100.0) * $s.Count)
    if ($rank -lt 1) { $rank = 1 }
    if ($rank -gt $s.Count) { $rank = $s.Count }
    return [double]$s[$rank - 1]
}

# Culture-proof formatting: on a Ukrainian/Russian Windows "-f" writes 1,5.
function Fmt([string]$f) {
    return [string]::Format([Globalization.CultureInfo]::InvariantCulture, $f, $args)
}

function Round3($x) {
    if ($null -eq $x) { return $null }
    return [math]::Round([double]$x, 3)
}

function New-Metric([string]$Id, $Samples, [string]$Method, [string]$Note = '', $Extra = $null) {
    $c = $Catalog[$Id]
    $xs = @()
    if ($null -ne $Samples) { $xs = @($Samples | ForEach-Object { [double]$_ }) }
    $stat = $c[2]
    $value = $null
    if ($xs.Count -gt 0) {
        switch ($stat) {
            'median' { $value = Get-Median $xs }
            'max' { $value = Get-Percentile $xs 100 }
            'p95' { $value = Get-Percentile $xs 95 }
        }
    }
    $status = 'info'
    if ($null -ne $value -and $null -ne $c[1]) {
        if ($value -le $c[1]) { $status = 'pass' } else { $status = 'fail' }
    }
    $m = [ordered]@{
        id      = $Id
        label   = $c[3]
        unit    = $c[0]
        samples = @($xs | ForEach-Object { Round3 $_ })
        median  = Round3 (Get-Median $xs)
        p95     = Round3 (Get-Percentile $xs 95)
        min     = $null
        max     = Round3 (Get-Percentile $xs 100)
        target  = $c[1]
        stat    = $stat
        value   = Round3 $value
        status  = $status
        method  = $Method
    }
    if ($xs.Count -gt 0) { $m.min = Round3 (($xs | Measure-Object -Minimum).Minimum) }
    if ($Note) { $m.note = $Note }
    if ($null -ne $Extra) { $m.extra = $Extra }
    return $m
}

function New-Skip([string]$Id, [string]$Why) {
    $m = New-Metric $Id @() ''
    $m.status = 'skip'
    $m.note = $Why
    return $m
}

# ---------------------------------------------------------------- the trading window
function Get-KyivNow {
    try {
        $tz = [TimeZoneInfo]::FindSystemTimeZoneById('FLE Standard Time')
    } catch {
        try { $tz = [TimeZoneInfo]::FindSystemTimeZoneById('Europe/Kyiv') } catch { return Get-Date }
    }
    return [TimeZoneInfo]::ConvertTime((Get-Date), $tz)
}

function Test-InTradingWindow([datetime]$t) {
    $m = $t.Hour * 60 + $t.Minute
    return ($m -ge (16 * 60 + 25)) -and ($m -lt (18 * 60 + 30))
}

# True while a run may go on: not in the window, and (unless -Force) not
# about to enter it within $MarginSeconds.
function Test-MayContinue([int]$MarginSeconds = 90) {
    if ($Force) { return $true }
    $now = Get-KyivNow
    if (Test-InTradingWindow $now) { return $false }
    return -not (Test-InTradingWindow ($now.AddSeconds($MarginSeconds)))
}

# True if [now, now + seconds] touches the window at any minute.
function Test-WouldCross([int]$Seconds) {
    $now = Get-KyivNow
    for ($t = 0; $t -le $Seconds; $t += 60) {
        if (Test-InTradingWindow ($now.AddSeconds($t))) { return $true }
    }
    return (Test-InTradingWindow ($now.AddSeconds($Seconds)))
}

# ---------------------------------------------------------------- native helper
$NativeSource = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public class DeskStartResult {
    public double LineMs = -1;
    public double WindowMs = -1;
    public string Line = "";
    public int Pid = 0;
    public bool Exited = false;
    public int ExitCode = 0;
    public bool TimedOut = false;
}

public class DeskIdleResult {
    public double CpuSeconds = 0;
    public double WallSeconds = 0;
    public double WorkingSetMB = 0;
    public double PrivateMB = 0;
    public int Processes = 0;
}

public static class DeskBenchNative {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct PROCESSENTRY32W {
        public uint dwSize; public uint cntUsage; public uint th32ProcessID; public IntPtr th32DefaultHeapID;
        public uint th32ModuleID; public uint cntThreads; public uint th32ParentProcessID; public int pcPriClassBase;
        public uint dwFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szExeFile;
    }
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct FILETIME { public uint Low, High; }
    [StructLayout(LayoutKind.Sequential)] class MEMORYSTATUSEX {
        public uint dwLength = (uint)Marshal.SizeOf(typeof(MEMORYSTATUSEX)); public uint dwMemoryLoad;
        public ulong ullTotalPhys; public ulong ullAvailPhys; public ulong ullTotalPageFile; public ulong ullAvailPageFile;
        public ulong ullTotalVirtual; public ulong ullAvailVirtual; public ulong ullAvailExtendedVirtual;
    }
    [StructLayout(LayoutKind.Sequential)] struct LUID { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct TOKEN_PRIVILEGES { public uint Count; public LUID Luid; public uint Attr; }
    public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);

    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern bool Process32FirstW(IntPtr h, ref PROCESSENTRY32W e);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern bool Process32NextW(IntPtr h, ref PROCESSENTRY32W e);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool GetSystemTimes(out FILETIME idle, out FILETIME kernel, out FILETIME user);
    [DllImport("kernel32.dll")] static extern bool GlobalMemoryStatusEx([In, Out] MEMORYSTATUSEX m);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAlloc(IntPtr a, UIntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFree(IntPtr a, UIntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr p, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool LookupPrivilegeValue(string sys, string name, out LUID luid);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool AdjustTokenPrivileges(IntPtr t, bool all, ref TOKEN_PRIVILEGES n, uint len, IntPtr prev, IntPtr ret);
    [DllImport("ntdll.dll")] static extern int NtSetSystemInformation(int cls, ref int info, int len);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("winmm.dll")] static extern uint timeBeginPeriod(uint ms);
    [DllImport("winmm.dll")] static extern uint timeEndPeriod(uint ms);

    static string[] protectedNames = new string[] { "tradingview" };

    // Root + every descendant, skipping TradingView and whatever it started:
    // the bench neither counts nor kills it, even if Desk launched it.
    public static int[] Tree(int root) {
        var parent = new Dictionary<int, int>();
        var name = new Dictionary<int, string>();
        IntPtr snap = CreateToolhelp32Snapshot(2, 0);
        if (snap == IntPtr.Zero || snap == new IntPtr(-1)) return new int[] { root };
        try {
            var e = new PROCESSENTRY32W();
            e.dwSize = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32W));
            if (Process32FirstW(snap, ref e)) {
                do {
                    parent[(int)e.th32ProcessID] = (int)e.th32ParentProcessID;
                    name[(int)e.th32ProcessID] = (e.szExeFile ?? "").ToLowerInvariant();
                } while (Process32NextW(snap, ref e));
            }
        } finally { CloseHandle(snap); }
        var outList = new List<int>();
        if (!parent.ContainsKey(root)) return outList.ToArray();
        outList.Add(root);
        for (int i = 0; i < outList.Count; i++) {
            int p = outList[i];
            foreach (var kv in parent) {
                if (kv.Value != p || kv.Key == p || outList.Contains(kv.Key)) continue;
                bool skip = false;
                foreach (var pn in protectedNames) if (name[kv.Key].StartsWith(pn)) skip = true;
                if (!skip) outList.Add(kv.Key);
            }
        }
        return outList.ToArray();
    }

    public static bool HasVisibleWindow(int[] pids) {
        var set = new HashSet<uint>();
        foreach (var p in pids) set.Add((uint)p);
        bool found = false;
        EnumWindows(delegate (IntPtr h, IntPtr l) {
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            if (!set.Contains(pid) || !IsWindowVisible(h)) return true;
            if (GetWindow(h, 4) != IntPtr.Zero) return true; // owned popup, not a main window
            RECT r;
            if (GetWindowRect(h, out r) && (r.Right - r.Left) > 50 && (r.Bottom - r.Top) > 50) { found = true; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    // lineKey: the stdout line to wait for (null = none); its arrival time is LineMs.
    public static DeskStartResult Start(string exe, string args, string[] env, string lineKey, bool waitWindow,
                                        bool waitExit, int timeoutMs, StringBuilder stderr) {
        bool wantLine = lineKey != null;
        var r = new DeskStartResult();
        var psi = new ProcessStartInfo(exe, args);
        psi.UseShellExecute = false;
        psi.WorkingDirectory = System.IO.Path.GetDirectoryName(exe);
        psi.RedirectStandardOutput = true;
        psi.RedirectStandardError = true;
        psi.RedirectStandardInput = false;
        if (env != null) for (int i = 0; i + 1 < env.Length; i += 2) psi.EnvironmentVariables[env[i]] = env[i + 1];
        var p = new Process();
        p.StartInfo = psi;
        var sw = new Stopwatch();
        var lineSeen = new ManualResetEvent(false);
        p.OutputDataReceived += delegate (object s, DataReceivedEventArgs e) {
            if (e.Data == null) return;
            if (wantLine && r.LineMs < 0 && e.Data.Contains(lineKey)) {
                r.LineMs = sw.Elapsed.TotalMilliseconds;
                r.Line = e.Data;
                lineSeen.Set();
            }
        };
        p.ErrorDataReceived += delegate (object s, DataReceivedEventArgs e) {
            if (e.Data == null || stderr == null) return;
            lock (stderr) { stderr.Append(DateTime.UtcNow.ToString("o")).Append(' ').Append(e.Data).Append('\n'); }
        };
        timeBeginPeriod(1);
        try {
            sw.Start();
            p.Start();
            r.Pid = p.Id;
            p.BeginOutputReadLine();
            p.BeginErrorReadLine();
            while (sw.ElapsedMilliseconds < timeoutMs) {
                bool lineDone = !wantLine || r.LineMs >= 0;
                if (waitWindow && r.WindowMs < 0 && HasVisibleWindow(Tree(p.Id))) r.WindowMs = sw.Elapsed.TotalMilliseconds;
                bool winDone = !waitWindow || r.WindowMs >= 0;
                if (p.HasExited) {
                    r.Exited = true;
                    if (!waitExit || lineDone) { p.WaitForExit(); break; }
                }
                if (lineDone && winDone && !waitExit) break;
                if (wantLine && !lineDone) lineSeen.WaitOne(1); else Thread.Sleep(1);
            }
            if (sw.ElapsedMilliseconds >= timeoutMs) r.TimedOut = true;
            if (p.HasExited) { r.Exited = true; r.ExitCode = p.ExitCode; }
        } finally { timeEndPeriod(1); }
        return r;
    }

    // CPU of the tree over `seconds`, sampled every second so processes that
    // start or end inside the window are counted for the time they ran.
    public static DeskIdleResult Idle(int root, int seconds) {
        var r = new DeskIdleResult();
        var first = new Dictionary<int, double>();
        var last = new Dictionary<int, double>();
        DateTime t0 = DateTime.Now;
        var sw = Stopwatch.StartNew();
        int[] pids = Tree(root);
        while (true) {
            pids = Tree(root);
            foreach (int pid in pids) {
                try {
                    var pr = Process.GetProcessById(pid);
                    double cpu = pr.TotalProcessorTime.TotalSeconds;
                    if (!first.ContainsKey(pid)) first[pid] = (pr.StartTime > t0) ? 0.0 : cpu;
                    last[pid] = cpu;
                } catch (Exception) { }
            }
            if (sw.Elapsed.TotalSeconds >= seconds) break;
            Thread.Sleep(1000);
        }
        r.WallSeconds = sw.Elapsed.TotalSeconds;
        foreach (var kv in last) r.CpuSeconds += kv.Value - first[kv.Key];
        foreach (int pid in pids) {
            try {
                var pr = Process.GetProcessById(pid);
                pr.Refresh();
                r.WorkingSetMB += pr.WorkingSet64 / 1048576.0;
                r.PrivateMB += pr.PrivateMemorySize64 / 1048576.0;
                r.Processes += 1;
            } catch (Exception) { }
        }
        return r;
    }

    public static double[] Memory(int root) {
        double ws = 0, pb = 0;
        foreach (int pid in Tree(root)) {
            try { var pr = Process.GetProcessById(pid); ws += pr.WorkingSet64 / 1048576.0; pb += pr.PrivateMemorySize64 / 1048576.0; }
            catch (Exception) { }
        }
        return new double[] { ws, pb };
    }

    // Ask the main windows to close, then terminate what is left, children first.
    public static void CloseTree(int root, int graceMs) {
        int[] pids = Tree(root);
        foreach (int pid in pids) {
            try { var pr = Process.GetProcessById(pid); if (pr.MainWindowHandle != IntPtr.Zero) pr.CloseMainWindow(); } catch (Exception) { }
        }
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < graceMs) {
            bool any = false;
            foreach (int pid in pids) { try { if (!Process.GetProcessById(pid).HasExited) any = true; } catch (Exception) { } }
            if (!any) return;
            Thread.Sleep(100);
        }
        for (int i = pids.Length - 1; i >= 0; i--) {
            try { var pr = Process.GetProcessById(pids[i]); if (!pr.HasExited) pr.Kill(); } catch (Exception) { }
        }
    }

    public static double BusyPercent(int ms) {
        FILETIME i0, k0, u0, i1, k1, u1;
        GetSystemTimes(out i0, out k0, out u0);
        Thread.Sleep(ms);
        GetSystemTimes(out i1, out k1, out u1);
        double idle = Ft(i1) - Ft(i0), total = (Ft(k1) - Ft(k0)) + (Ft(u1) - Ft(u0));
        if (total <= 0) return 0;
        return (total - idle) / total * 100.0;
    }
    static double Ft(FILETIME f) { return ((ulong)f.High << 32 | f.Low) / 1e7; }

    public static double[] MemoryStatus() {
        var m = new MEMORYSTATUSEX();
        GlobalMemoryStatusEx(m);
        return new double[] { m.ullTotalPhys / 1048576.0, m.ullAvailPhys / 1048576.0 };
    }

    // Admin only: empty the standby list (the file cache), the same call
    // RAMMap's "Empty Standby List" makes. Returns NTSTATUS (0 = done).
    public static int PurgeStandby() {
        IntPtr tok;
        if (!OpenProcessToken(GetCurrentProcess(), 0x28, out tok)) return -1;
        try {
            var tp = new TOKEN_PRIVILEGES();
            tp.Count = 1; tp.Attr = 2;
            if (!LookupPrivilegeValue(null, "SeProfileSingleProcessPrivilege", out tp.Luid)) return -2;
            if (!AdjustTokenPrivileges(tok, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero) || Marshal.GetLastWin32Error() != 0) return -3;
        } finally { CloseHandle(tok); }
        int cmd = 3; // MemoryFlushModifiedList
        NtSetSystemInformation(80, ref cmd, 4);
        cmd = 4;     // MemoryPurgeStandbyList
        return NtSetSystemInformation(80, ref cmd, 4);
    }

    // No admin: commit and touch memory until available RAM (free + standby)
    // is down to `floorMB`, then free it. Standby pages - the cached exe and
    // DLLs among them - are repurposed for it. Returns {MB taken, seconds}.
    public static double[] Pressure(int floorMB, int maxSeconds) {
        var blocks = new List<IntPtr>();
        UIntPtr chunk = new UIntPtr(64UL << 20);
        var sw = Stopwatch.StartNew();
        double taken = 0;
        try {
            while (sw.Elapsed.TotalSeconds < maxSeconds) {
                double[] st = MemoryStatus();
                if (st[1] < floorMB + 64 || taken > st[0]) break;
                IntPtr p = VirtualAlloc(IntPtr.Zero, chunk, 0x3000, 0x04);
                if (p == IntPtr.Zero) break;
                blocks.Add(p);
                for (long off = 0; off < (64L << 20); off += 4096) Marshal.WriteByte(p, (int)off, 1);
                taken += 64;
            }
        } finally {
            foreach (var p in blocks) VirtualFree(p, UIntPtr.Zero, 0x8000);
        }
        return new double[] { taken, sw.Elapsed.TotalSeconds };
    }
}
'@

# ---------------------------------------------------------------- environment
function Get-MachineInfo {
    $m = [ordered]@{ os = ''; cpu = ''; logical_cpus = [Environment]::ProcessorCount; ram_gb = 0; power = 'unknown'; power_plan = ''; noise = @() }
    try {
        $os = Get-CimInstance Win32_OperatingSystem
        $m.os = ('{0} {1} (build {2})' -f $os.Caption, $os.Version, $os.BuildNumber)
        $m.ram_gb = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
    } catch { $m.os = 'Windows' }
    try { $m.cpu = ((Get-CimInstance Win32_Processor | Select-Object -First 1).Name).Trim() } catch { }
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $ps = [System.Windows.Forms.SystemInformation]::PowerStatus
        if ($ps.PowerLineStatus -eq 'Online') { $m.power = 'ac' } elseif ($ps.PowerLineStatus -eq 'Offline') { $m.power = 'battery' }
    } catch { }
    try {
        $plan = Get-CimInstance -Namespace root\cimv2\power -ClassName Win32_PowerPlan -Filter 'IsActive=True'
        if ($plan) { $m.power_plan = [string]$plan.ElementName }
    } catch { }
    return $m
}

function Get-Noise {
    $n = New-Object System.Collections.ArrayList
    $busy = [DeskBenchNative]::BusyPercent(3000)
    [void]$n.Add((Fmt 'cpu busy before start: {0:F1} %' $busy))
    if ($busy -gt 10) { [void]$n.Add('WARNING: machine busy before the run (> 10 % CPU), numbers will be noisy') }
    # Read-only look at the process list: TradingView running adds noise.
    if (Get-Process -Name 'TradingView*' -ErrorAction SilentlyContinue) { [void]$n.Add('TradingView is running (not touched)') }
    return , $n.ToArray()
}

function Find-ElectronExe {
    $roots = @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall')
    foreach ($r in $roots) {
        if (-not (Test-Path $r)) { continue }
        foreach ($k in Get-ChildItem $r -ErrorAction SilentlyContinue) {
            $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
            if ($null -eq $p) { continue }
            $dn = $p.PSObject.Properties['DisplayName']
            $il = $p.PSObject.Properties['InstallLocation']
            if ($dn -and $il -and ([string]$dn.Value) -like 'Strategy Desk*') {
                $cand = Join-Path ([string]$il.Value) 'Strategy Desk.exe'
                if (Test-Path $cand) { return $cand }
            }
        }
    }
    $base = Join-Path $env:LOCALAPPDATA 'Programs'
    $hits = @(Get-ChildItem -Path $base -Filter 'Strategy Desk.exe' -Recurse -Depth 2 -ErrorAction SilentlyContinue)
    if ($hits.Count -gt 0) { return $hits[0].FullName }
    return $null
}

# The vault the Rust build is pointed at (DESK_VAULT). Order: -Vault, the
# environment, the folder the installed Electron desk is set to, the default
# Documents\Strategy Desk. Returns @(path, source); the report keeps only the source.
function Resolve-Vault {
    $cands = New-Object System.Collections.ArrayList
    if ($Vault) { [void]$cands.Add(@($Vault, '-Vault')) }
    if ($env:DESK_VAULT) { [void]$cands.Add(@($env:DESK_VAULT, 'env DESK_VAULT')) }
    $cfg = ''
    if ($env:APPDATA) { $cfg = Join-Path $env:APPDATA 'strategy-desk\config.json' }
    if ($cfg -and (Test-Path -LiteralPath $cfg)) {
        try {
            $c = Get-Content -Raw -Encoding UTF8 -LiteralPath $cfg | ConvertFrom-Json
            $vp = $c.PSObject.Properties['vaultPath']
            if ($vp -and ([string]$vp.Value).Trim()) { [void]$cands.Add(@(([string]$vp.Value).Trim(), 'electron settings')) }
        } catch { }
    }
    $docs = [Environment]::GetFolderPath('MyDocuments')
    if ($docs) { [void]$cands.Add(@((Join-Path $docs 'Strategy Desk'), 'documents default')) }
    foreach ($c in $cands) {
        if (Test-Path -LiteralPath (Join-Path $c[0] 'indices-kb\state')) { return @($c[0], $c[1]) }
        if ($c[1] -eq '-Vault') { throw ('-Vault "{0}" has no indices-kb\state folder' -f $c[0]) }
    }
    return @('', 'none')
}

# `powershell -File` passes -Variants a,b as ONE string "a,b"; a PowerShell
# prompt passes an array. Both end up as a list here (so a value cannot hold a comma).
function Expand-VariantArgs([string[]]$Items) {
    $out = New-Object System.Collections.ArrayList
    foreach ($it in $Items) { foreach ($x in ($it -split ',')) { if ($x.Trim()) { [void]$out.Add($x.Trim()) } } }
    return , $out.ToArray()
}

# "[label|]KEY=VALUE[;KEY=VALUE...]"; the key "exe" names another binary.
function ConvertTo-Variant([string]$Spec, [int]$Index, [string]$DefaultExe) {
    $label = $Spec
    $body = $Spec
    $bar = $Spec.IndexOf('|')
    if ($bar -ge 0) { $label = $Spec.Substring(0, $bar).Trim(); $body = $Spec.Substring($bar + 1) }
    $envMap = [ordered]@{}
    $exePath = $DefaultExe
    foreach ($part in ($body -split ';')) {
        $kv = $part.Trim()
        if (-not $kv) { continue }
        $eq = $kv.IndexOf('=')
        if ($eq -lt 1) { throw ('variant "{0}": "{1}" is not KEY=VALUE' -f $Spec, $kv) }
        $k = $kv.Substring(0, $eq).Trim()
        $v = $kv.Substring($eq + 1).Trim()
        if ($k -eq 'exe') { $exePath = $v } else { $envMap[$k] = $v }
    }
    if (-not $label) { $label = ('v{0}' -f $Index) }
    return [ordered]@{ id = ('v{0}' -f $Index); label = $label; env = $envMap; exe = $exePath; mode = '' }
}

# {"first_frame_ms": .., "data_ready_ms": .., "private_mb": .., "ws_mb": ..}; missing keys = $null.
function Read-BenchLine([string]$Line) {
    $o = @{ first_frame_ms = $null; data_ready_ms = $null; private_mb = $null; ws_mb = $null }
    if (-not $Line) { return $o }
    try { $j = ConvertFrom-Json $Line } catch { return $o }
    foreach ($k in @('first_frame_ms', 'data_ready_ms', 'private_mb', 'ws_mb')) {
        $p = $j.PSObject.Properties[$k]
        if ($p -and $null -ne $p.Value) { $o[$k] = [double]$p.Value }
    }
    return $o
}

# The data-ready line arrives when the data is drawn. The first frame on the
# outside clock is that arrival minus what the process says lay between the
# two: both of its numbers come from one clock, so the difference is exact.
function Get-DerivedFirstFrame($LineMs, $SelfFirst, $SelfReady) {
    if ($null -eq $LineMs -or $LineMs -lt 0 -or $null -eq $SelfFirst -or $null -eq $SelfReady) { return $null }
    return $LineMs - ($SelfReady - $SelfFirst)
}

function Get-DirMB([string]$dir) {
    $sum = (Get-ChildItem -LiteralPath $dir -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
    if ($null -eq $sum) { return 0 }
    return $sum / 1MB
}

function Get-FrameStats([string]$text) {
    # DESK_FRAME_STATS=1 lines (desk-ui, every 10 s on stderr, today
    # "frame_stats n=600 p50=4.20ms p95=.. p99=.. max=.."). Tolerant: any line
    # that names p50/p95/p99 counts; frames = "frames"/"n"/"count".
    $rows = New-Object System.Collections.ArrayList
    foreach ($line in ($text -split "`n")) {
        $m50 = [regex]::Match($line, 'p50[^0-9.]{0,12}([0-9]+(\.[0-9]+)?)')
        if (-not $m50.Success) { continue }
        $m95 = [regex]::Match($line, 'p95[^0-9.]{0,12}([0-9]+(\.[0-9]+)?)')
        $m99 = [regex]::Match($line, 'p99[^0-9.]{0,12}([0-9]+(\.[0-9]+)?)')
        $mn = [regex]::Match($line, '(?<![A-Za-z_])(frames|n|count)"?\s*[:=]\s*([0-9]+)')
        $row = [ordered]@{
            p50    = [double]::Parse($m50.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
            p95    = $null; p99 = $null; frames = $null
        }
        if ($m95.Success) { $row.p95 = [double]::Parse($m95.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture) }
        if ($m99.Success) { $row.p99 = [double]::Parse($m99.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture) }
        if ($mn.Success) { $row.frames = [int]$mn.Groups[2].Value }
        [void]$rows.Add($row)
    }
    return , $rows.ToArray()
}

function Add-FrameMetrics($list, [string]$prefix, $rows, [string]$method) {
    [void]$list.Add((New-Metric ($prefix + 'p50_ms') @($rows | ForEach-Object { $_.p50 }) $method))
    [void]$list.Add((New-Metric ($prefix + 'p95_ms') @($rows | Where-Object { $null -ne $_.p95 } | ForEach-Object { $_.p95 }) $method))
    [void]$list.Add((New-Metric ($prefix + 'p99_ms') @($rows | Where-Object { $null -ne $_.p99 } | ForEach-Object { $_.p99 }) $method))
}

function Write-Report($report, [string]$path) {
    $dir = Split-Path -Parent $path
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $json = ConvertTo-Json -InputObject $report -Depth 12
    [IO.File]::WriteAllText($path, $json + "`n", (New-Object Text.UTF8Encoding $false))
    Write-Host ('wrote {0}' -f $path)
}

function Show-Summary($report) {
    foreach ($m in $report.metrics) {
        $v = '-'
        if ($null -ne $m.value) { $v = Fmt '{0:F1}' $m.value }
        $t = '-'
        if ($null -ne $m.target) { $t = [string]$m.target }
        Write-Host ('{0,-26} {1,10} {2,-5} target {3,6}  {4}' -f $m.id, $v, $m.unit, $t, $m.status)
    }
    $vs = @()
    if ($report.Contains('variants')) { $vs = @($report.variants) }
    if ($vs.Count -gt 0) {
        Write-Host ''
        Write-Host ('variants: ' + (($vs | ForEach-Object { '{0}={1}' -f $_.id, $_.label }) -join ', '))
        foreach ($id in @('cold_start_ms', 'warm_start_ms', 'data_ready_cold_ms', 'data_ready_warm_ms')) {
            $cells = foreach ($v in $vs) {
                $m = @($v.metrics | Where-Object { $_.id -eq $id })
                if ($m.Count -gt 0 -and $null -ne $m[0].value) { Fmt '{0,10:F1} {1,-4}' $m[0].value $m[0].status } else { '         -     ' }
            }
            Write-Host (('{0,-20}' -f $id) + ($cells -join ' '))
        }
    }
}

function New-Report([string]$kindName, $machine, $build, $metrics, $notes, [string]$coldMethod, $variants, [string]$vaultSource) {
    $now = Get-Date
    $r = [ordered]@{
        schema         = 'desk-perf/1'
        representative = $true
        label          = $Label
        created_at     = $now.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        date           = $now.ToString('yyyy-MM-dd')
        machine        = $machine
        build          = $build
        harness        = [ordered]@{
            script         = ('scripts/rust/bench.ps1 v{0}' -f $ScriptVersion)
            runs           = $Runs
            cold_method    = $coldMethod
            idle_seconds   = $IdleSeconds
            settle_seconds = $SettleSeconds
            vault          = $vaultSource
        }
        metrics        = @($metrics)
        notes          = @($notes)
    }
    if ($null -ne $variants -and @($variants).Count -gt 0) { $r.variants = @($variants) }
    return $r
}

# ---------------------------------------------------------------- self-test
if ($SelfTest) {
    # Pure parts only, so this also runs under pwsh on Linux (CI, agents).
    $fail = 0
    function Assert-Eq($a, $b, [string]$what) {
        if ("$a" -ne "$b") { Write-Host ('FAIL {0}: {1} != {2}' -f $what, $a, $b); $script:fail++ } else { Write-Host ('ok   {0}' -f $what) }
    }
    Assert-Eq (Fmt '{0:F1} ms' 1.5) '1.5 ms' 'invariant format'
    Assert-Eq (Get-Median @(3, 1, 2)) 2 'median odd'
    Assert-Eq (Get-Median @(4, 1, 2, 3)) 2.5 'median even'
    Assert-Eq (Get-Percentile @(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) 95) 10 'p95 nearest rank'
    Assert-Eq (Get-Percentile @(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) 50) 5 'p50 nearest rank'
    Assert-Eq (New-Metric 'warm_start_ms' @(100, 120, 400) 't').status 'pass' 'median judged'
    Assert-Eq (New-Metric 'frame_p95_ms' @(10, 12, 40) 't').status 'fail' 'worst window judged'
    Assert-Eq (New-Metric 'switch_ms' @(10, 20, 30, 40, 50, 60, 70, 80, 90, 120) 't').status 'fail' 'switch p95'
    Assert-Eq (New-Metric 'data_ready_cold_ms' @(450, 480, 700) 't').status 'pass' 'data ready cold'
    Assert-Eq (New-Metric 'data_ready_warm_ms' @(310, 320, 200) 't').status 'fail' 'data ready warm'
    Assert-Eq (New-Metric 'install_dir_mb' @(5) 't').status 'info' 'no target = info'
    Assert-Eq (New-Skip 'cold_start_ms' 'x').status 'skip' 'skip'
    Assert-Eq (Test-InTradingWindow ([datetime]'2026-11-25 16:25')) $true 'window start'
    Assert-Eq (Test-InTradingWindow ([datetime]'2026-11-25 18:30')) $false 'window end'
    Assert-Eq (Test-InTradingWindow ([datetime]'2026-11-25 16:24')) $false 'before window'
    $fs = Get-FrameStats ('2026-11-25T10:00:00Z {"frame_stats":{"frames":600,"p50_ms":4.2,"p95_ms":9.1,"p99_ms":15.5}}' + "`n" + 'noise' + "`n" + 'frame_stats n=10 p50=1.50ms p95=2.00ms p99=3.25ms max=9.00ms')
    Assert-Eq $fs.Count 2 'frame lines'
    Assert-Eq $fs[0].p95 9.1 'frame p95'
    Assert-Eq $fs[1].frames 10 'frame count (desk-ui format)'
    Assert-Eq $fs[1].p99 3.25 'frame p99 (desk-ui format)'
    $v = ConvertTo-Variant 'glow|DESK_OPT=all; exe=C:\b\glow.exe' 2 'C:\a\desk.exe'
    Assert-Eq $v.label 'glow' 'variant label'
    Assert-Eq $v.exe 'C:\b\glow.exe' 'variant exe'
    Assert-Eq $v.env['DESK_OPT'] 'all' 'variant env'
    Assert-Eq (Expand-VariantArgs @('DESK_OPT=base,DESK_OPT=all')).Count 2 'variants from -File'
    Assert-Eq (Expand-VariantArgs @('a=1', 'b=2;c=3')).Count 2 'variants from a prompt'
    $v = ConvertTo-Variant 'DESK_OPT=base' 1 'C:\a\desk.exe'
    Assert-Eq $v.label 'DESK_OPT=base' 'variant default label'
    Assert-Eq $v.exe 'C:\a\desk.exe' 'variant default exe'
    $b = Read-BenchLine '{"first_frame_ms": 180.5, "data_ready_ms": 260.0, "private_mb": 95.1, "ws_mb": 80}'
    Assert-Eq $b.data_ready_ms 260 'bench line data ready'
    Assert-Eq (Get-DerivedFirstFrame 300.0 $b.first_frame_ms $b.data_ready_ms) 220.5 'derived first frame'
    Assert-Eq (Get-DerivedFirstFrame 300.0 180.5 $null) '' 'no data ready = no derivation'
    Assert-Eq (Read-BenchLine '{"first_frame_ms": 1.0, "rss_mb": null}').data_ready_ms '' 'old line has no data ready'
    $machine = [ordered]@{ os = 'selftest'; cpu = ''; logical_cpus = 1; ram_gb = 0; power = 'unknown'; power_plan = ''; noise = @('self-test') }
    $build = [ordered]@{ kind = $Kind; version = '0'; commit = 'selftest'; exe = 'none'; profile = '' }
    $vars = @([ordered]@{ id = 'v1'; label = 'base'; env = [ordered]@{ DESK_OPT = 'base' }; exe = 'desk.exe'; mode = 'data-ready'; metrics = @((New-Metric 'warm_start_ms' @(120) 't')) })
    $r = New-Report $Kind $machine $build @((New-Metric 'exe_mb' @(4.2) 'size'), (New-Skip 'idle_cpu_pct' 'self-test')) @('self-test') 'none' $vars 'none'
    $r.representative = $false
    $path = $Out
    if (-not $path) { $path = Join-Path ([IO.Path]::GetTempPath()) 'desk-bench-selftest.json' }
    Write-Report $r $path
    $back = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
    Assert-Eq $back.schema 'desk-perf/1' 'round trip schema'
    Assert-Eq @($back.metrics).Count 2 'round trip metrics'
    Assert-Eq @($back.metrics[0].samples).Count 1 'single sample stays an array'
    Assert-Eq @($back.variants).Count 1 'single variant stays an array'
    Assert-Eq @($back.variants[0].metrics).Count 1 'variant metrics stay an array'
    Assert-Eq $back.harness.vault 'none' 'vault source'
    if ($fail -gt 0) { exit 1 }
    Write-Host 'self-test passed'
    exit 0
}

# ---------------------------------------------------------------- guards
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'bench.ps1 measures Windows only; use -SelfTest elsewhere.' }
if (-not [Environment]::Is64BitProcess) { throw 'Run the 64-bit Windows PowerShell (the memory-pressure cold method needs it).' }
$kyiv = Get-KyivNow
if ((Test-InTradingWindow $kyiv) -and -not $Force) {
    Write-Host ('Refusing: it is {0:HH:mm} in Kyiv, inside the 16:25-18:30 trading window. Run later, or pass -Force.' -f $kyiv)
    exit 2
}

if (-not $Exe) {
    if ($Kind -eq 'electron') { $Exe = Find-ElectronExe } else { $Exe = Join-Path $RepoRoot 'rust\target\release\strategy-desk.exe' }
}
if (-not $Exe -or -not (Test-Path -LiteralPath $Exe)) {
    throw ('exe not found: "{0}". Pass -Exe <path>.' -f $Exe)
}
$Exe = (Resolve-Path -LiteralPath $Exe).Path

# Variants: the first one is the reference and fills `metrics`; without
# -Variants there is exactly one, the plain exe with no extra environment.
$Variants = Expand-VariantArgs $Variants
$variantList = New-Object System.Collections.ArrayList
if ($Variants.Count -eq 0) {
    [void]$variantList.Add((ConvertTo-Variant 'default|' 1 $Exe))
} else {
    for ($i = 0; $i -lt $Variants.Count; $i++) { [void]$variantList.Add((ConvertTo-Variant $Variants[$i] ($i + 1) $Exe)) }
}
foreach ($v in $variantList) {
    if (-not (Test-Path -LiteralPath $v.exe)) { throw ('variant {0}: exe not found: "{1}"' -f $v.label, $v.exe) }
    $v.exe = (Resolve-Path -LiteralPath $v.exe).Path
    $pn = [IO.Path]::GetFileNameWithoutExtension($v.exe)
    $running = @(Get-Process -Name $pn -ErrorAction SilentlyContinue)
    if ($running.Count -gt 0) {
        Write-Host ('Refusing: {0} is already running ({1} processes). Close Strategy Desk first; the bench never closes a window it did not open.' -f (Split-Path -Leaf $v.exe), $running.Count)
        exit 2
    }
}
$Exe = $variantList[0].exe   # sizes and the build line describe the reference
$exeName = Split-Path -Leaf $Exe
$VR = $VariantRuns
if ($VR -le 0) { $VR = $Runs }

if ($SoakHours -gt 0) {
    # A soak is its own run: nothing else in it, so the series is clean.
    $SkipCold = [switch]$true; $SkipWarm = [switch]$true; $SkipIdle = [switch]$true
}

Add-Type -TypeDefinition $NativeSource -Language CSharp
$isAdmin = $false
try { $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) } catch { }
if ($ColdMethod -eq 'auto') { if ($isAdmin) { $ColdMethod = 'admin' } else { $ColdMethod = 'pressure' } }
if ($ColdMethod -eq 'admin' -and -not $isAdmin) { throw '-ColdMethod admin needs an elevated PowerShell.' }

# Rough duration, so a run that would reach 16:25 is refused up front.
$nv = $variantList.Count
$coldPer = 8
if ($ColdMethod -eq 'pressure') { $coldPer = 45 }
$est = $nv * 8
if (-not $SkipCold) { $est += $nv * $VR * $coldPer }
if (-not $SkipWarm) { $est += $nv * ($VR + 1) * 4 }
if (-not $SkipIdle) { $est += $IdleRuns * ($IdleSeconds + $SettleSeconds + 15) }
if ($StreamFrames) { $est += 90 }
if ($SoakHours -gt 0) { $est = [int]($SoakHours * 3600) }
Write-Host (Fmt 'bench.ps1: about {0:F0} min ({1} variant(s) x {2} runs, cold method {3})' ($est / 60.0) $nv $VR $ColdMethod)
if (-not $Force -and $SoakHours -le 0 -and (Test-WouldCross ($est + 60))) {
    Write-Host ('Refusing: the run takes about {0} min and would reach the 16:25 trading window. Start after 18:30, or pass -Force.' -f [math]::Ceiling($est / 60))
    exit 2
}

$vaultPath = ''
$vaultSource = 'electron settings (its own)'
if ($Kind -eq 'rust') {
    $vr2 = Resolve-Vault
    $vaultPath = $vr2[0]; $vaultSource = $vr2[1]
    if (-not $vaultPath) { Write-Host 'WARNING: no vault found (-Vault, DESK_VAULT, the Electron settings, Documents\Strategy Desk): the Rust build will start on an empty screen and data_ready cannot be measured.' }
    else { Write-Host ('vault: {0}' -f $vaultSource) }
}

$machine = Get-MachineInfo
$machine.noise = Get-Noise
$ver = ''
try { $ver = [Diagnostics.FileVersionInfo]::GetVersionInfo($Exe).ProductVersion } catch { }
if (-not $Commit) {
    if ($Kind -eq 'electron') { $Commit = ('electron-{0}' -f $ver) }
    else {
        try { $Commit = (& git -C $RepoRoot rev-parse --short HEAD 2>$null) } catch { }
        if (-not $Commit) { $Commit = 'unknown' }
    }
}
if (-not $Label) { $Label = ('{0} {1}' -f $Kind, $ver).Trim() }
$build = [ordered]@{ kind = $Kind; version = $ver; commit = [string]$Commit; exe = $exeName; profile = 'release' }
if (-not $Out) { $Out = Join-Path $RepoRoot ('docs\rust\perf\{0}-{1}.json' -f (Get-Date -Format 'yyyy-MM-dd'), $Commit) }

$metrics = New-Object System.Collections.ArrayList
$notes = New-Object System.Collections.ArrayList
$aborted = $false
Write-Host 'Do not touch the mouse or keyboard while it runs; windows will open and close by themselves.'
$timeoutMs = $StartTimeoutSeconds * 1000

# Environment pairs for one launch: the vault (Rust only), the variant, extras.
function Get-EnvPairs($v, [string[]]$extra) {
    $pairs = New-Object System.Collections.ArrayList
    if ($Kind -eq 'rust' -and $vaultPath) { [void]$pairs.Add('DESK_VAULT'); [void]$pairs.Add($vaultPath) }
    foreach ($k in $v.env.Keys) { [void]$pairs.Add([string]$k); [void]$pairs.Add([string]$v.env[$k]) }
    if ($null -ne $extra) { foreach ($x in $extra) { [void]$pairs.Add($x) } }
    if ($pairs.Count -eq 0) { return $null }
    return [string[]]$pairs.ToArray()
}

function Invoke-Start($v, [string]$what, [string]$modeOverride = '') {
    $mode = $v.mode
    if ($modeOverride) { $mode = $modeOverride }
    $err = New-Object System.Text.StringBuilder
    $startArgs = ''
    $lineKey = $null
    if ($Kind -eq 'rust') {
        $lineKey = 'first_frame_ms'
        if ($mode -eq 'data-ready') { $startArgs = '--bench-data-ready' } else { $startArgs = '--bench-first-frame' }
    }
    # Rust: wait for the line (the process then exits by itself); Electron:
    # wait for the first visible window. Whatever is still running is closed.
    $r = [DeskBenchNative]::Start($v.exe, $startArgs, (Get-EnvPairs $v $null), $lineKey, $true, $false, $timeoutMs, $err)
    if (-not $r.Exited) { [DeskBenchNative]::CloseTree($r.Pid, 5000) }
    Start-Sleep -Milliseconds 1500  # let the processes go and the disk settle
    $b = Read-BenchLine $r.Line
    $line = $null
    if ($r.LineMs -ge 0) { $line = $r.LineMs }
    $primary = $r.WindowMs
    $ready = $null
    if ($Kind -eq 'rust') {
        if ($mode -eq 'data-ready' -and $null -ne $b.data_ready_ms) {
            $primary = Get-DerivedFirstFrame $line $b.first_frame_ms $b.data_ready_ms
            $ready = $line
        } else { $primary = $line }
    }
    if ($null -ne $primary -and $primary -lt 0) {
        # The process claims more time between its first frame and the data
        # than passed outside: its clock is wrong, so the sample is dropped.
        Write-Host ('WARNING: {0}: self-reported first_frame/data_ready do not fit the outside clock; sample dropped' -f $what)
        $primary = $null
    }
    if ($null -eq $primary) { $primary = -1 }
    $msg = Fmt '{0}: first frame {1:F1} ms, window {2:F1} ms' $what $primary $r.WindowMs
    if ($null -ne $ready) { $msg += (Fmt ', data ready {0:F1} ms' $ready) }
    if ($r.TimedOut) { $msg += ' (TIMED OUT)' }
    Write-Host $msg
    return @{ primary = $primary; window = $r.WindowMs; self = $b.first_frame_ms; ready = $ready; selfReady = $b.data_ready_ms
        private_mb = $b.private_mb; ws_mb = $b.ws_mb; timedOut = $r.TimedOut; line = $r.Line; stderr = $err.ToString() }
}

function New-StartMetric([string]$id, $results, [string]$field, [string]$method) {
    $ok = @($results | Where-Object { -not $_.timedOut -and $null -ne $_[$field] -and $_[$field] -ge 0 })
    $extra = [ordered]@{ timeouts = @($results | Where-Object { $_.timedOut }).Count }
    if ($field -eq 'primary') {
        $extra.window_ms = @($results | ForEach-Object { Round3 $_.window })
        if ($Kind -eq 'rust') { $extra.self_reported_ms = @($results | ForEach-Object { Round3 $_.self }) }
    } else {
        $extra.self_reported_ms = @($results | ForEach-Object { Round3 $_.selfReady })
        $extra.private_mb_at_ready = @($results | ForEach-Object { Round3 $_.private_mb })
        $extra.ws_mb_at_ready = @($results | ForEach-Object { Round3 $_.ws_mb })
    }
    $note = ''
    if ($extra.timeouts -gt 0) { $note = ('{0} launch(es) timed out after {1} s and are not in samples' -f $extra.timeouts, $StartTimeoutSeconds) }
    return (New-Metric $id @($ok | ForEach-Object { $_[$field] }) $method $note $extra)
}

# ---------------------------------------------------------------- which start protocol each variant speaks
foreach ($v in $variantList) {
    if ($Kind -ne 'rust') { $v.mode = 'window'; continue }
    $probe = Invoke-Start $v ('probe {0} (discarded)' -f $v.label) 'data-ready'
    if ($null -ne $probe.selfReady) { $v.mode = 'data-ready' }
    else {
        $v.mode = 'first-frame'
        $why = 'no {"data_ready_ms"} line'
        if (-not $vaultPath) { $why += ' (no vault)' }
        if ($probe.stderr -match 'bench-data-ready') { $why += ' (this build does not know --bench-data-ready)' }
        [void]$notes.Add(('variant {0}: {1}; start measured with --bench-first-frame' -f $v.label, $why))
    }
}

# ---------------------------------------------------------------- cold + warm, variants interleaved
# Interleaving (v1, v2, v1, v2, ...) spreads drift - heat, a background
# update - over every variant instead of piling it on the last one.
$cold = @{}; $warm = @{}
foreach ($v in $variantList) { $cold[$v.id] = New-Object System.Collections.ArrayList; $warm[$v.id] = New-Object System.Collections.ArrayList }
$flushNotes = New-Object System.Collections.ArrayList
if (-not $SkipCold) {
    for ($i = 1; $i -le $VR -and -not $aborted; $i++) {
        foreach ($v in $variantList) {
            if (-not (Test-MayContinue 120)) { $aborted = $true; break }
            if ($ColdMethod -eq 'admin') {
                [void]$flushNotes.Add(('purge status {0}' -f [DeskBenchNative]::PurgeStandby()))
            } elseif ($ColdMethod -eq 'pressure') {
                $pr = [DeskBenchNative]::Pressure(256, 90)
                [void]$flushNotes.Add((Fmt 'pressure {0:F0} MB in {1:F1} s' $pr[0] $pr[1]))
            }
            Start-Sleep -Seconds 3
            [void]$cold[$v.id].Add((Invoke-Start $v ('cold {0}/{1} {2}' -f $i, $VR, $v.label)))
        }
    }
}
if (-not $SkipWarm -and -not $aborted) {
    foreach ($v in $variantList) { [void](Invoke-Start $v ('warm-up {0} (discarded)' -f $v.label)) }
    for ($i = 1; $i -le $VR -and -not $aborted; $i++) {
        foreach ($v in $variantList) {
            if (-not (Test-MayContinue 60)) { $aborted = $true; break }
            [void]$warm[$v.id].Add((Invoke-Start $v ('warm {0}/{1} {2}' -f $i, $VR, $v.label)))
        }
    }
}

$variantsOut = New-Object System.Collections.ArrayList
foreach ($v in $variantList) {
    $vm = New-Object System.Collections.ArrayList
    $sm = 'Stopwatch from Process.Start to the first visible top-level window of the process tree (EnumWindows, 1 ms polling)'
    $dm = ''
    if ($v.mode -eq 'first-frame') { $sm = 'Stopwatch from Process.Start to the {"first_frame_ms"} line on stdout (--bench-first-frame); window_ms = same clock to the first visible window' }
    if ($v.mode -eq 'data-ready') {
        $sm = 'Stopwatch from Process.Start to the --bench-data-ready line, minus the (data_ready_ms - first_frame_ms) the process reports on its own clock = start to first frame on the outside clock; window_ms = same clock to the first visible window'
        $dm = ('Stopwatch from Process.Start to the {{"data_ready_ms"}} line on stdout (--bench-data-ready): the day of the vault ({0}) drawn' -f $vaultSource)
    }
    $pairs = @(@('cold_start_ms', $cold, 'primary', ('{0}; before each launch: {1}' -f $sm, $ColdMethod)),
        @('warm_start_ms', $warm, 'primary', ('{0}; right after a previous launch' -f $sm)),
        @('data_ready_cold_ms', $cold, 'ready', ('{0}; before each launch: {1}' -f $dm, $ColdMethod)),
        @('data_ready_warm_ms', $warm, 'ready', ('{0}; right after a previous launch' -f $dm)))
    foreach ($p in $pairs) {
        $res = $p[1][$v.id]
        $isCold = $p[0] -like '*cold*'
        if (($isCold -and $SkipCold) -or (-not $isCold -and $SkipWarm)) { [void]$vm.Add((New-Skip $p[0] ('skipped by -Skip' + @('Warm', 'Cold')[[int]$isCold]))); continue }
        if ($p[2] -eq 'ready' -and $v.mode -ne 'data-ready') {
            $why = 'Electron 0.2.3 has no hook that says when the day data is drawn; see the manual stopwatch method in docs/rust/PERF.md'
            if ($Kind -eq 'rust') { $why = 'this build printed no data_ready_ms (see notes)' }
            [void]$vm.Add((New-Skip $p[0] $why)); continue
        }
        if ($res.Count -eq 0) { [void]$vm.Add((New-Skip $p[0] 'stopped before the first sample (trading window)')); continue }
        $m = New-StartMetric $p[0] $res.ToArray() $p[2] $p[3]
        if ($isCold -and $flushNotes.Count -gt 0) { $m.extra.flush = @($flushNotes) }
        [void]$vm.Add($m)
    }
    [void]$variantsOut.Add([ordered]@{ id = $v.id; label = $v.label; env = $v.env; exe = (Split-Path -Leaf $v.exe); mode = $v.mode; metrics = @($vm.ToArray()) })
}
foreach ($m in $variantsOut[0].metrics) { [void]$metrics.Add($m) }
$ref = $variantList[0]

# ---------------------------------------------------------------- idle (reference variant)
if (-not $SkipIdle -and -not $aborted) {
    $ws = New-Object System.Collections.ArrayList
    $pb = New-Object System.Collections.ArrayList
    $cpu = New-Object System.Collections.ArrayList
    $fpm = New-Object System.Collections.ArrayList
    $procs = New-Object System.Collections.ArrayList
    for ($i = 1; $i -le $IdleRuns; $i++) {
        if (-not (Test-MayContinue ($IdleSeconds + $SettleSeconds + 60))) { $aborted = $true; break }
        $err = New-Object System.Text.StringBuilder
        $extraEnv = $null
        if ($Kind -eq 'rust') { $extraEnv = [string[]]@('DESK_FRAME_STATS', '1') }
        $r = [DeskBenchNative]::Start($ref.exe, '', (Get-EnvPairs $ref $extraEnv), $null, $true, $false, $timeoutMs, $err)
        if ($r.WindowMs -lt 0) {
            [void]$notes.Add(('idle run {0}: no window within {1} s' -f $i, $StartTimeoutSeconds))
            [DeskBenchNative]::CloseTree($r.Pid, 5000)
            continue
        }
        Start-Sleep -Seconds $SettleSeconds
        $t0 = [DateTime]::UtcNow
        $idle = [DeskBenchNative]::Idle($r.Pid, $IdleSeconds)
        $t1 = [DateTime]::UtcNow
        [DeskBenchNative]::CloseTree($r.Pid, 5000)
        Start-Sleep -Seconds 2
        $pct = $idle.CpuSeconds / $idle.WallSeconds / [Environment]::ProcessorCount * 100.0
        [void]$ws.Add($idle.WorkingSetMB); [void]$pb.Add($idle.PrivateMB); [void]$cpu.Add($pct); [void]$procs.Add($idle.Processes)
        # Frame lines stamped inside the idle window only.
        $inWin = New-Object System.Text.StringBuilder
        foreach ($line in ($err.ToString() -split "`n")) {
            if ($line.Length -lt 28) { continue }
            $ts = [DateTime]::MinValue
            if ([DateTime]::TryParse($line.Substring(0, 28), [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::RoundtripKind, [ref]$ts)) {
                $ts = $ts.ToUniversalTime()
                if ($ts -ge $t0 -and $ts -le $t1) { [void]$inWin.Append($line).Append("`n") }
            }
        }
        $rows = Get-FrameStats $inWin.ToString()
        $frames = @($rows | Where-Object { $null -ne $_.frames } | ForEach-Object { $_.frames })
        if ($frames.Count -gt 0) { [void]$fpm.Add((($frames | Measure-Object -Sum).Sum) / ($IdleSeconds / 60.0)) }
        Write-Host (Fmt 'idle {0}/{1}: WS {2:F1} MB, private {3:F1} MB, CPU {4:F2} %, {5} processes' $i $IdleRuns $idle.WorkingSetMB $idle.PrivateMB $pct $idle.Processes)
    }
    $im = ('variant {0}; launch, first visible window, {1} s settle (not counted), then {2} s idle; memory = sum over the process tree at the end' -f $ref.label, $SettleSeconds, $IdleSeconds)
    if ($ws.Count -gt 0) {
        [void]$metrics.Add((New-Metric 'idle_working_set_mb' $ws.ToArray() ($im + ', Process.WorkingSet64') '' ([ordered]@{ processes = @($procs) })))
        [void]$metrics.Add((New-Metric 'idle_private_mb' $pb.ToArray() ($im + ', Process.PrivateMemorySize64 (= Private Bytes)')))
        [void]$metrics.Add((New-Metric 'idle_cpu_pct' $cpu.ToArray() ('TotalProcessorTime of the tree sampled every 1 s over the idle window / wall / logical CPUs (Task Manager scale)')))
    } else {
        foreach ($id in @('idle_working_set_mb', 'idle_private_mb', 'idle_cpu_pct')) { [void]$metrics.Add((New-Skip $id 'no idle run completed')) }
    }
    if ($Kind -eq 'rust' -and $fpm.Count -gt 0) {
        [void]$metrics.Add((New-Metric 'idle_frames_per_min' $fpm.ToArray() 'sum of "frames" in DESK_FRAME_STATS=1 stderr lines inside the idle window'))
    } elseif ($Kind -eq 'rust') {
        [void]$metrics.Add((New-Skip 'idle_frames_per_min' 'no DESK_FRAME_STATS line with a frame count inside the idle window'))
    } else {
        [void]$metrics.Add((New-Skip 'idle_frames_per_min' 'Electron has no frame counter'))
    }
} else {
    foreach ($id in @('idle_working_set_mb', 'idle_private_mb', 'idle_cpu_pct')) { [void]$metrics.Add((New-Skip $id '-SkipIdle')) }
}

# ---------------------------------------------------------------- frames
$frameIds = @('frame_p50_ms', 'frame_p95_ms', 'frame_p99_ms')
$switchIds = @('switch_frame_p50_ms', 'switch_frame_p95_ms', 'switch_frame_p99_ms', 'switch_ms')
if ($StreamFrames -and $Kind -eq 'rust' -and -not $aborted) {
    # (a) The chat stream: fake-claude as claude (desk-app honours
    # DESK_CLAUDE_BIN) and one message sent by desk-ui itself
    # (DESK_BENCH_AUTOCHAT). Until stage 5 that second hook is a stub that
    # says so on stderr; then this part is reported as waiting, not guessed.
    if (-not $FakeClaude) { $FakeClaude = Join-Path $RepoRoot 'rust\target\release\fake-claude.exe' }
    if (-not (Test-Path -LiteralPath $FakeClaude)) {
        foreach ($id in $frameIds) { [void]$metrics.Add((New-Skip $id ('fake-claude not found: {0} (cargo build --release -p desk-bench)' -f (Split-Path -Leaf $FakeClaude)))) }
    } else {
        $err = New-Object System.Text.StringBuilder
        $extraEnv = [string[]]@('DESK_FRAME_STATS', '1', 'DESK_CLAUDE_BIN', $FakeClaude, 'DESK_BENCH_AUTOCHAT', '1',
            'FAKE_CLAUDE_EVENTS', [string]$StreamEvents, 'FAKE_CLAUDE_RATE', (Fmt '{0}' $StreamRate))
        $r = [DeskBenchNative]::Start($ref.exe, '', (Get-EnvPairs $ref $extraEnv), $null, $true, $false, $timeoutMs, $err)
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $state = 'waiting'
        while ($sw.Elapsed.TotalSeconds -lt 20 -and $state -eq 'waiting') {
            Start-Sleep -Milliseconds 500
            if ($err.ToString() -match 'DESK_BENCH_AUTOCHAT=1 ignored') { $state = 'stub'; break }
            $kids = @([DeskBenchNative]::Tree($r.Pid))
            if (@(Get-Process -Id $kids -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like 'fake-claude*' }).Count -gt 0) { $state = 'streaming' }
        }
        if ($state -eq 'streaming') {
            Start-Sleep -Seconds ([int]($StreamEvents / [math]::Max($StreamRate, 1)) + 25)
        }
        [DeskBenchNative]::CloseTree($r.Pid, 5000)
        $rows = Get-FrameStats $err.ToString()
        if ($state -eq 'streaming' -and $rows.Count -gt 0) {
            Add-FrameMetrics $metrics 'frame_' $rows ('DESK_FRAME_STATS=1 per-10-s windows while fake-claude streams {0} events at {1}/s; p95/p99 judged on the worst window' -f $StreamEvents, $StreamRate)
        } else {
            $why = 'waits for stage 5: DESK_BENCH_AUTOCHAT is still a stub in this build, so no chat is sent (see switch_frame_* for frames under load)'
            if ($state -eq 'waiting') { $why = 'fake-claude never started within 20 s: the build sent no chat message' }
            if ($state -eq 'streaming') { $why = 'fake-claude ran but no DESK_FRAME_STATS line came out' }
            foreach ($id in $frameIds) { [void]$metrics.Add((New-Skip $id $why)) }
        }
    }

    # (b) Under load today: --bench-switch flips days as fast as it can (the
    # stand-in for scrolling), DESK_FRAME_STATS reports every 10 s, and the
    # same run gives switch_ms (command to the frame with the new day).
    if (-not $vaultPath) {
        foreach ($id in $switchIds) { [void]$metrics.Add((New-Skip $id 'needs a vault with two days or more (-Vault)')) }
    } elseif (-not (Test-MayContinue 240)) {
        $aborted = $true
    } else {
        $err = New-Object System.Text.StringBuilder
        $r = [DeskBenchNative]::Start($ref.exe, ('--bench-switch {0}' -f $SwitchCount), (Get-EnvPairs $ref ([string[]]@('DESK_FRAME_STATS', '1'))), 'switch_ms', $false, $false, 180000, $err)
        if (-not $r.Exited) { [DeskBenchNative]::CloseTree($r.Pid, 5000) }
        $rows = Get-FrameStats $err.ToString()
        $sm = ('variant {0}; strategy-desk --bench-switch {1} with DESK_FRAME_STATS=1 on the vault ({2})' -f $ref.label, $SwitchCount, $vaultSource)
        if ($rows.Count -gt 0) {
            Add-FrameMetrics $metrics 'switch_frame_' $rows ($sm + '; per-10-s windows, p95/p99 judged on the worst window')
        } else {
            foreach ($id in $switchIds[0..2]) { [void]$metrics.Add((New-Skip $id 'no DESK_FRAME_STATS line in the --bench-switch run: shorter than one 10-s window (raise -SwitchCount) or the build failed')) }
        }
        $times = @()
        $timeout = $false
        if ($r.Line) {
            try {
                $j = ConvertFrom-Json $r.Line
                $times = @($j.switch_ms | ForEach-Object { [double]$_ })
                if ($j.PSObject.Properties['timeout'] -and $j.timeout) { $timeout = $true }
            } catch { }
        }
        if ($times.Count -gt 0) {
            $note = ''
            if ($timeout) { $note = 'the build reported a switch that took longer than its 10 s timeout' }
            [void]$metrics.Add((New-Metric 'switch_ms' $times ($sm + '; each switch: the Command to the start of the frame after the one that drew the new day (desk-ui clock)') $note))
        } else {
            [void]$metrics.Add((New-Skip 'switch_ms' 'no {"switch_ms"} line (does this build know --bench-switch, and does the vault have two days?)'))
        }
    }
} else {
    $why = 'not requested (-StreamFrames)'
    if ($Kind -eq 'electron') { $why = 'Electron has no frame counter' }
    foreach ($id in ($frameIds + $switchIds)) { [void]$metrics.Add((New-Skip $id $why)) }
}

# ---------------------------------------------------------------- sizes
[void]$metrics.Add((New-Metric 'exe_mb' @((Get-Item -LiteralPath $Exe).Length / 1MB) ('size of {0}' -f $exeName)))
$exeDir = Split-Path -Parent $Exe
if ($exeDir -like (Join-Path $env:LOCALAPPDATA 'Programs\*')) {
    [void]$metrics.Add((New-Metric 'install_dir_mb' @(Get-DirMB $exeDir) 'every file under the installation folder'))
} else {
    [void]$metrics.Add((New-Skip 'install_dir_mb' 'exe is not under %LOCALAPPDATA%\Programs (a build folder is not an installation)'))
}
if (-not $Installer -and $Kind -eq 'electron' -and $ver) {
    $cand = @(Get-ChildItem -Path (Join-Path $RepoRoot 'release') -Filter ('Strategy-Desk-{0}-*.exe' -f $ver) -ErrorAction SilentlyContinue)
    if ($cand.Count -gt 0) { $Installer = $cand[0].FullName }
}
if ($Installer -and (Test-Path -LiteralPath $Installer)) {
    [void]$metrics.Add((New-Metric 'installer_mb' @((Get-Item -LiteralPath $Installer).Length / 1MB) ('size of {0}' -f (Split-Path -Leaf $Installer))))
} else {
    [void]$metrics.Add((New-Skip 'installer_mb' 'no installer found; pass -Installer <path>'))
}

# ---------------------------------------------------------------- soak
if ($SoakHours -gt 0) {
    $err = New-Object System.Text.StringBuilder
    $r = [DeskBenchNative]::Start($ref.exe, '', (Get-EnvPairs $ref $null), $null, $true, $false, $timeoutMs, $err)
    $series = New-Object System.Collections.ArrayList
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $base = $null
    while ($sw.Elapsed.TotalHours -lt $SoakHours) {
        # The soak ends before 16:25 by itself: the trading window is not for benches.
        if (-not (Test-MayContinue ($SoakIntervalMinutes * 60 + 120))) { $aborted = $true; break }
        Start-Sleep -Seconds ($SoakIntervalMinutes * 60)
        $mem = [DeskBenchNative]::Memory($r.Pid)
        if ($mem[1] -le 0) { [void]$notes.Add('soak: the process tree is gone'); break }
        [void]$series.Add([ordered]@{ min = [math]::Round($sw.Elapsed.TotalMinutes, 1); private_mb = Round3 $mem[1]; ws_mb = Round3 $mem[0] })
        # Baseline at >= 10 min, after the start-up allocations settled.
        if ($null -eq $base -and $sw.Elapsed.TotalMinutes -ge 10) { $base = $mem[1] }
    }
    [DeskBenchNative]::CloseTree($r.Pid, 5000)
    if ($null -ne $base -and $series.Count -gt 0) {
        $lastPb = $series[$series.Count - 1].private_mb
        $growth = ($lastPb - $base) / $base * 100.0
        $hours = [math]::Round($sw.Elapsed.TotalHours, 2)
        $note = ''
        if ($hours -lt 24) { $note = ('measured over {0} h, not 24 h (the soak stops before 16:25)' -f $hours) }
        [void]$metrics.Add((New-Metric 'soak_private_growth_pct' @($growth) ('Private Bytes of the tree every {0} min; growth = last vs. the first sample after 10 min' -f $SoakIntervalMinutes) $note ([ordered]@{ series = @($series) ; hours = $hours })))
    } else {
        [void]$metrics.Add((New-Skip 'soak_private_growth_pct' 'soak ended before a 10-minute baseline'))
    }
} else {
    [void]$metrics.Add((New-Skip 'soak_private_growth_pct' 'not requested (-SoakHours 24)'))
}

foreach ($id in @('day_load_ms', 'watch_latency_ms')) {
    [void]$notes.Add(('{0}: measured by desk-bench (cargo bench / perf-sample), not by this script' -f $id))
}
if ($aborted) { [void]$notes.Add('ABORTED: stopped early to stay out of the 16:25-18:30 trading window') }

$variantsForReport = $null
if ($Variants.Count -gt 0) { $variantsForReport = $variantsOut.ToArray() }
$report = New-Report $Kind $machine $build $metrics.ToArray() $notes.ToArray() $ColdMethod $variantsForReport $vaultSource
Write-Report $report $Out
Show-Summary $report
