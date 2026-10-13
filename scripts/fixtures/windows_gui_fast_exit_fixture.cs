using System;
using System.Diagnostics;
using System.IO;
using System.Threading.Tasks;

namespace Assemblywright.Windows
{
    internal static class WindowsGuiFastExitFixture
    {
        private const int Rounds = 20;
        private const int ChildrenPerRound = 16;

        private static int Main(string[] arguments)
        {
            try
            {
                if (arguments.Length > 0 && arguments[0] == "--child") return RunChild(arguments);
                RunHarness();
                return 0;
            }
            catch (Exception exception)
            {
                Console.Error.WriteLine(exception.ToString());
                return 1;
            }
        }

        private static int RunChild(string[] arguments)
        {
            Require(arguments.Length == 4, "The fast-exit child received invalid arguments.");
            string marker = arguments[1];
            int exitCode = int.Parse(arguments[2], System.Globalization.CultureInfo.InvariantCulture);
            bool terminateWithNewline = arguments[3] == "newline";
            if (terminateWithNewline)
            {
                Console.Out.WriteLine("stdout|" + marker);
                Console.Error.WriteLine("stderr|" + marker);
            }
            else
            {
                Console.Out.Write("stdout|" + marker);
                Console.Error.Write("stderr|" + marker);
            }
            Console.Out.Flush();
            Console.Error.Flush();
            return exitCode;
        }

        private static void RunHarness()
        {
            string executable;
            using (Process current = Process.GetCurrentProcess()) executable = current.MainModule.FileName;
            string childExecutable = Path.Combine(Path.GetDirectoryName(executable), "fast-exit-child-" + Guid.NewGuid().ToString("N") + ".exe");
            File.Copy(executable, childExecutable);
            int successes = 0;
            int failures = 0;
            try
            {
                int warmupSuccesses = 0;
                int warmupFailures = 0;
                RunCaptureBatches(childExecutable, 0, 4, ref warmupSuccesses, ref warmupFailures);
                Require(warmupSuccesses == 32 && warmupFailures == 32, "The capture warmup did not validate every child.");
                GC.Collect();
                GC.WaitForPendingFinalizers();
                int initialHandles;
                using (Process current = Process.GetCurrentProcess()) initialHandles = current.HandleCount;
                RunCaptureBatches(childExecutable, 64, Rounds, ref successes, ref failures);
                GC.Collect();
                GC.WaitForPendingFinalizers();
                int finalHandles;
                using (Process current = Process.GetCurrentProcess()) finalHandles = current.HandleCount;
                Require(
                    finalHandles <= initialHandles + 32,
                    "Fast-exit capture leaked process handles (initial=" + initialHandles.ToString(System.Globalization.CultureInfo.InvariantCulture)
                        + ", final=" + finalHandles.ToString(System.Globalization.CultureInfo.InvariantCulture) + ").");
            }
            finally
            {
                File.Delete(childExecutable);
            }
            Require(!File.Exists(childExecutable), "A completed fast-exit child still held its executable open.");

            bool launchFailureObserved = false;
            string missing = Path.Combine(Path.GetTempPath(), "assemblywright-missing-" + Guid.NewGuid().ToString("N") + ".exe");
            try
            {
                ProcessRunner.RunCapturedAsync(missing, new string[0]).GetAwaiter().GetResult();
            }
            catch
            {
                launchFailureObserved = true;
            }
            Require(launchFailureObserved, "A missing executable did not return a launch failure.");
            Require(successes == 160 && failures == 160, "The harness did not validate every success and failure result.");
            Console.Out.WriteLine("PASS iterations=320 successful=160 failed=160 launch_failure=observed");
        }

        private static void RunCaptureBatches(string executable, int firstSequence, int rounds, ref int successes, ref int failures)
        {
            for (int round = 0; round < rounds; round++)
            {
                Task<ProcessResult>[] captures = new Task<ProcessResult>[ChildrenPerRound];
                string[] markers = new string[ChildrenPerRound];
                int[] exitCodes = new int[ChildrenPerRound];
                Task[] waits = new Task[ChildrenPerRound];
                for (int child = 0; child < ChildrenPerRound; child++)
                {
                    int sequence = firstSequence + round * ChildrenPerRound + child;
                    string marker = sequence.ToString(System.Globalization.CultureInfo.InvariantCulture);
                    int exitCode = sequence % 2 == 0 ? 0 : 23;
                    string lineMode = sequence % 3 == 0 ? "none" : "newline";
                    markers[child] = marker;
                    exitCodes[child] = exitCode;
                    captures[child] = ProcessRunner.RunCapturedAsync(
                        executable,
                        new string[] { "--child", marker, exitCode.ToString(System.Globalization.CultureInfo.InvariantCulture), lineMode });
                    waits[child] = captures[child];
                }
                Require(Task.WaitAll(waits, 15000), "A fast-exit capture did not complete within 15 seconds.");
                for (int child = 0; child < ChildrenPerRound; child++)
                {
                    ProcessResult result = captures[child].GetAwaiter().GetResult();
                    string expectedOutput = "stdout|" + markers[child] + Environment.NewLine;
                    string expectedError = "stderr|" + markers[child] + Environment.NewLine;
                    Require(result.ExitCode == exitCodes[child], "A fast-exit child returned the wrong exit code.");
                    Require(result.StandardOutput == expectedOutput, "A fast-exit child lost or changed stdout.");
                    Require(result.StandardError == expectedError, "A fast-exit child lost or changed stderr.");
                    if (result.ExitCode == 0) successes++; else failures++;
                }
            }
        }

        private static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
    }
}
