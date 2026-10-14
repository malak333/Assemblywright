using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;

// Native E2E fixture: pause one Git preflight at a real process boundary.
internal static class GitPreflightFixture
{
    private static int Main(string[] args)
    {
        string realGit = Environment.GetEnvironmentVariable("AW_GUI_TEST_REAL_GIT");
        string marker = Environment.GetEnvironmentVariable("AW_GUI_TEST_PREFLIGHT_MARKER");
        string release = Environment.GetEnvironmentVariable("AW_GUI_TEST_PREFLIGHT_RELEASE");
        if (string.IsNullOrEmpty(realGit) || !File.Exists(realGit)) return 91;
        if (args.Length >= 5 && args[0] == "-C" && args[2] == "rev-parse"
            && !string.IsNullOrEmpty(marker) && !File.Exists(marker))
        {
            File.WriteAllText(marker, "preflight waiting");
            DateTime deadline = DateTime.UtcNow.AddSeconds(30);
            while (!File.Exists(release))
            {
                if (DateTime.UtcNow >= deadline) return 92;
                Thread.Sleep(50);
            }
        }
        StringBuilder command = new StringBuilder();
        foreach (string arg in args)
        {
            if (command.Length > 0) command.Append(' ');
            command.Append(Quote(arg));
        }
        ProcessStartInfo start = new ProcessStartInfo(realGit, command.ToString());
        start.UseShellExecute = false;
        start.CreateNoWindow = true;
        using (Process child = Process.Start(start))
        {
            child.WaitForExit();
            return child.ExitCode;
        }
    }

    private static string Quote(string arg)
    {
        StringBuilder quoted = new StringBuilder("\"");
        int slashes = 0;
        foreach (char character in arg)
        {
            if (character == '\\') { slashes++; continue; }
            quoted.Append('\\', character == '"' ? slashes * 2 + 1 : slashes);
            quoted.Append(character);
            slashes = 0;
        }
        quoted.Append('\\', slashes * 2);
        return quoted.Append('"').ToString();
    }
}
