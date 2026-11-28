// Linux stand-in for DeskBenchNative (bench-sim.sh): real processes and
// pipes, pretend windows, memory and cache. Same public surface.
using System;
using System.Diagnostics;
using System.Text;
using System.Threading;

public class DeskStartResult {
    public double LineMs = -1; public double WindowMs = -1; public string Line = ""; public int Pid = 0;
    public bool Exited = false; public int ExitCode = 0; public bool TimedOut = false;
}
public class DeskIdleResult {
    public double CpuSeconds = 0.06; public double WallSeconds = 1; public double WorkingSetMB = 75; public double PrivateMB = 88; public int Processes = 1;
}
public static class DeskBenchNative {
    // Root + descendants from /proc/<pid>/stat (field 4 = ppid).
    public static int[] Tree(int root) {
        var parent = new System.Collections.Generic.Dictionary<int, int>();
        foreach (var d in System.IO.Directory.GetDirectories("/proc")) {
            int pid;
            if (!int.TryParse(System.IO.Path.GetFileName(d), out pid)) continue;
            try {
                string st = System.IO.File.ReadAllText(d + "/stat");
                string[] f = st.Substring(st.LastIndexOf(')') + 2).Split(' ');
                parent[pid] = int.Parse(f[1]);
            } catch (Exception) { }
        }
        var outList = new System.Collections.Generic.List<int>();
        outList.Add(root);
        for (int i = 0; i < outList.Count; i++)
            foreach (var kv in parent) if (kv.Value == outList[i] && !outList.Contains(kv.Key)) outList.Add(kv.Key);
        return outList.ToArray();
    }
    public static DeskStartResult Start(string exe, string args, string[] env, string lineKey, bool waitWindow,
                                        bool waitExit, int timeoutMs, StringBuilder stderr) {
        if (string.IsNullOrEmpty(lineKey)) lineKey = null;   // PowerShell passes $null as ""
        var r = new DeskStartResult();
        var psi = new ProcessStartInfo(exe, args);
        psi.UseShellExecute = false; psi.RedirectStandardOutput = true; psi.RedirectStandardError = true;
        if (env != null) for (int i = 0; i + 1 < env.Length; i += 2) psi.Environment[env[i]] = env[i + 1];
        var p = new Process(); p.StartInfo = psi;
        var sw = new Stopwatch();
        p.OutputDataReceived += delegate (object s, DataReceivedEventArgs e) {
            if (e.Data != null && lineKey != null && r.LineMs < 0 && e.Data.Contains(lineKey)) { r.LineMs = sw.Elapsed.TotalMilliseconds; r.Line = e.Data; }
        };
        p.ErrorDataReceived += delegate (object s, DataReceivedEventArgs e) {
            if (e.Data == null || stderr == null) return;
            lock (stderr) { stderr.Append(DateTime.UtcNow.ToString("o")).Append(' ').Append(e.Data).Append('\n'); }
        };
        sw.Start(); p.Start(); r.Pid = p.Id; p.BeginOutputReadLine(); p.BeginErrorReadLine();
        while (sw.ElapsedMilliseconds < timeoutMs) {
            if (waitWindow && r.WindowMs < 0 && sw.ElapsedMilliseconds > 30) r.WindowMs = sw.Elapsed.TotalMilliseconds;
            bool lineDone = lineKey == null || r.LineMs >= 0;
            bool winDone = !waitWindow || r.WindowMs >= 0;
            if (p.HasExited) { r.Exited = true; p.WaitForExit(); break; }
            if (lineDone && winDone && !waitExit) break;
            Thread.Sleep(1);
        }
        if (sw.ElapsedMilliseconds >= timeoutMs) r.TimedOut = true;
        if (p.HasExited) { r.Exited = true; r.ExitCode = p.ExitCode; }
        return r;
    }
    public static DeskIdleResult Idle(int root, int seconds) { Thread.Sleep(seconds * 1000); var r = new DeskIdleResult(); r.WallSeconds = seconds; return r; }
    public static double[] Memory(int root) { return new double[] { 75, 88 }; }
    public static void CloseTree(int root, int graceMs) {
        int[] t = Tree(root);
        for (int i = t.Length - 1; i >= 0; i--) { try { Process.GetProcessById(t[i]).Kill(); } catch (Exception) { } }
    }
    public static double BusyPercent(int ms) { return 1.5; }
    public static double[] MemoryStatus() { return new double[] { 16000, 8000 }; }
    public static int PurgeStandby() { return 0; }
    public static double[] Pressure(int floorMB, int maxSeconds) { return new double[] { 0, 0 }; }
}
