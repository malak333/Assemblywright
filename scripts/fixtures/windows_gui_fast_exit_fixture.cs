using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Assemblywright.Windows
{
    internal static class WindowsGuiFastExitFixture
    {
        private const int Rounds = 20;
        private const int ChildrenPerRound = 16;
        private const int HandleTolerance = 32;
        private const int StabilizationRounds = Rounds;
        private const int MaximumStabilizationWindows = 4;
        private const int RequiredStableWindows = 2;

        private sealed class RuntimeSample
        {
            internal int Handles;
            internal int Threads;
        }

        private sealed class RuntimeStabilizationState
        {
            private readonly RuntimeSample[] recent = new RuntimeSample[RequiredStableWindows + 1];
            private int recentCount;
            private int peakHandles;
            private int peakThreads;
            private int windowsWithoutNewPeak;

            internal RuntimeStabilizationState(RuntimeSample initial)
            {
                peakHandles = initial.Handles;
                peakThreads = initial.Threads;
                AddRecent(initial);
            }

            internal bool Observe(RuntimeSample current)
            {
                bool establishedNewPeak = current.Handles > peakHandles || current.Threads > peakThreads;
                peakHandles = Math.Max(peakHandles, current.Handles);
                peakThreads = Math.Max(peakThreads, current.Threads);
                if (establishedNewPeak) windowsWithoutNewPeak = 0;
                else windowsWithoutNewPeak++;
                AddRecent(current);
                return IsStable;
            }

            internal bool IsStable
            {
                get { return windowsWithoutNewPeak >= RequiredStableWindows && RecentRangeIsBounded(); }
            }

            private void AddRecent(RuntimeSample sample)
            {
                if (recentCount < recent.Length)
                {
                    recent[recentCount++] = sample;
                    return;
                }
                for (int index = 1; index < recent.Length; index++) recent[index - 1] = recent[index];
                recent[recent.Length - 1] = sample;
            }

            private bool RecentRangeIsBounded()
            {
                if (recentCount < recent.Length) return false;
                int minimumHandles = recent[0].Handles;
                int maximumHandles = recent[0].Handles;
                int minimumThreads = recent[0].Threads;
                int maximumThreads = recent[0].Threads;
                for (int index = 1; index < recentCount; index++)
                {
                    minimumHandles = Math.Min(minimumHandles, recent[index].Handles);
                    maximumHandles = Math.Max(maximumHandles, recent[index].Handles);
                    minimumThreads = Math.Min(minimumThreads, recent[index].Threads);
                    maximumThreads = Math.Max(maximumThreads, recent[index].Threads);
                }
                return maximumHandles - minimumHandles <= HandleTolerance
                    && maximumThreads - minimumThreads <= ChildrenPerRound / 2;
            }
        }

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
            VerifyStabilizationDecisionLogic();
            string executable;
            using (Process current = Process.GetCurrentProcess()) executable = current.MainModule.FileName;
            string childExecutable = Path.Combine(Path.GetDirectoryName(executable), "fast-exit-child-" + Guid.NewGuid().ToString("N") + ".exe");
            File.Copy(executable, childExecutable);
            int successes = 0;
            int failures = 0;
            int originalMinimumWorkers;
            int originalMinimumIo;
            ThreadPool.GetMinThreads(out originalMinimumWorkers, out originalMinimumIo);
            try
            {
                PrimeThreadPool(originalMinimumWorkers, originalMinimumIo);
                int warmupSuccesses = 0;
                int warmupFailures = 0;
                RunCaptureBatches(childExecutable, 0, 4, ref warmupSuccesses, ref warmupFailures);
                Require(warmupSuccesses == 32 && warmupFailures == 32, "The capture warmup did not validate every child.");
                RuntimeSample[] stabilizationSamples;
                RuntimeSample initial = StabilizeRuntime(childExecutable, out stabilizationSamples);
                RunCaptureBatches(childExecutable, 64, Rounds, ref successes, ref failures);
                RuntimeSample final = CaptureRuntimeSample();
                Console.Out.WriteLine(
                    "DIAGNOSTICS stabilization=" + FormatSamples(stabilizationSamples)
                        + " baseline_handles=" + Invariant(initial.Handles)
                        + " baseline_threads=" + Invariant(initial.Threads)
                        + " final_handles=" + Invariant(final.Handles)
                        + " final_threads=" + Invariant(final.Threads)
                        + " tolerance=" + Invariant(HandleTolerance));
                Require(
                    HandlesWithinBound(initial, final),
                    "Fast-exit capture leaked process handles (initial=" + Invariant(initial.Handles)
                        + ", final=" + Invariant(final.Handles) + ").");

                VerifyRetainedProcessHandleDetection();
            }
            finally
            {
                ThreadPool.SetMinThreads(originalMinimumWorkers, originalMinimumIo);
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
            Console.Out.WriteLine("PASS iterations=320 successful=160 failed=160 launch_failure=observed retained_handle_negative_control=rejected");
        }

        private static void PrimeThreadPool(int originalMinimumWorkers, int originalMinimumIo)
        {
            int target = Math.Max(ChildrenPerRound * 2, originalMinimumWorkers);
            Require(
                ThreadPool.SetMinThreads(target, Math.Max(ChildrenPerRound * 2, originalMinimumIo)),
                "The fixture could not set its temporary ThreadPool minimums.");
            CountdownEvent started = new CountdownEvent(target);
            ManualResetEventSlim release = new ManualResetEventSlim(false);
            Task[] work = new Task[target];
            for (int index = 0; index < target; index++)
            {
                work[index] = Task.Factory.StartNew(delegate
                {
                    started.Signal();
                    release.Wait();
                }, CancellationToken.None, TaskCreationOptions.None, TaskScheduler.Default);
            }
            bool allStarted = started.Wait(15000);
            release.Set();
            bool drained = Task.WaitAll(work, 15000);
            if (drained)
            {
                release.Dispose();
                started.Dispose();
            }
            Require(drained, "Queued ThreadPool priming work did not finish within 15 seconds.");
            Require(allStarted, "Queued ThreadPool priming work did not start within 15 seconds.");
        }

        private static RuntimeSample StabilizeRuntime(string executable, out RuntimeSample[] samples)
        {
            RuntimeSample[] observed = new RuntimeSample[MaximumStabilizationWindows + 1];
            int count = 0;
            observed[count++] = CaptureRuntimeSample();
            RuntimeStabilizationState state = new RuntimeStabilizationState(observed[0]);
            for (int window = 0; window < MaximumStabilizationWindows && !state.IsStable; window++)
            {
                int successes = 0;
                int failures = 0;
                RunCaptureBatches(
                    executable,
                    10000 + window * StabilizationRounds * ChildrenPerRound,
                    StabilizationRounds,
                    ref successes,
                    ref failures);
                Require(
                    successes == StabilizationRounds * ChildrenPerRound / 2
                        && failures == StabilizationRounds * ChildrenPerRound / 2,
                    "A runtime stabilization window did not validate every child.");
                RuntimeSample current = CaptureRuntimeSample();
                observed[count++] = current;
                state.Observe(current);
            }
            samples = new RuntimeSample[count];
            Array.Copy(observed, samples, count);
            if (!state.IsStable)
            {
                Console.Out.WriteLine("DIAGNOSTICS stabilization=" + FormatSamples(samples) + " stabilization_failed=true");
                throw new InvalidOperationException("The CLR and ThreadPool did not stabilize within the bounded fixture warmup.");
            }
            return samples[count - 1];
        }

        private static void VerifyStabilizationDecisionLogic()
        {
            RuntimeSample[] hosted = new RuntimeSample[] {
                Sample(439, 48), Sample(449, 50), Sample(484, 57), Sample(484, 57), Sample(484, 57)
            };
            Require(FirstStableWindow(hosted) == 4, "The hosted runtime sequence did not stabilize on its fourth window.");
            RuntimeSample[] persistentGrowth = new RuntimeSample[] {
                Sample(100, 10), Sample(101, 10), Sample(102, 10), Sample(103, 10), Sample(104, 10)
            };
            Require(FirstStableWindow(persistentGrowth) == -1, "Persistent handle growth was accepted as stabilization.");
            RuntimeSample[] unboundedRange = new RuntimeSample[] {
                Sample(200, 20), Sample(100, 10), Sample(160, 16), Sample(100, 10), Sample(160, 16)
            };
            Require(FirstStableWindow(unboundedRange) == -1, "An unbounded recent runtime range was accepted as stabilization.");
        }

        private static int FirstStableWindow(RuntimeSample[] samples)
        {
            RuntimeStabilizationState state = new RuntimeStabilizationState(samples[0]);
            for (int index = 1; index < samples.Length; index++)
            {
                if (state.Observe(samples[index])) return index;
            }
            return -1;
        }

        private static RuntimeSample Sample(int handles, int threads)
        {
            RuntimeSample sample = new RuntimeSample();
            sample.Handles = handles;
            sample.Threads = threads;
            return sample;
        }

        private static RuntimeSample CaptureRuntimeSample()
        {
            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();
            Thread.Sleep(100);
            using (Process current = Process.GetCurrentProcess())
            {
                RuntimeSample sample = new RuntimeSample();
                sample.Handles = current.HandleCount;
                sample.Threads = current.Threads.Count;
                return sample;
            }
        }

        private static void VerifyRetainedProcessHandleDetection()
        {
            RuntimeSample baseline = CaptureRuntimeSample();
            Process[] retained = new Process[HandleTolerance + 1];
            RuntimeSample withRetained;
            try
            {
                for (int index = 0; index < retained.Length; index++)
                {
                    retained[index] = Process.GetCurrentProcess();
                    IntPtr handle = retained[index].Handle;
                    Require(handle != IntPtr.Zero, "A retained-process handle could not be opened.");
                }
                withRetained = CaptureRuntimeSample();
            }
            finally
            {
                foreach (Process process in retained)
                {
                    if (process != null) process.Dispose();
                }
            }
            RuntimeSample released = CaptureRuntimeSample();
            bool rejected = !HandlesWithinBound(baseline, withRetained);
            bool recovered = HandlesWithinBound(baseline, released);
            Console.Out.WriteLine(
                "NEGATIVE_CONTROL baseline_handles=" + Invariant(baseline.Handles)
                    + " retained_handles=" + Invariant(withRetained.Handles)
                    + " released_handles=" + Invariant(released.Handles)
                    + " rejected=" + rejected.ToString().ToLowerInvariant()
                    + " recovered=" + recovered.ToString().ToLowerInvariant());
            Require(rejected, "The process-handle leak detector accepted deliberately retained process handles.");
            Require(recovered, "The deliberately retained process handles were not released after disposal.");
        }

        private static bool HandlesWithinBound(RuntimeSample baseline, RuntimeSample observed)
        {
            return observed.Handles <= baseline.Handles + HandleTolerance;
        }

        private static string FormatSamples(RuntimeSample[] samples)
        {
            StringBuilder text = new StringBuilder();
            foreach (RuntimeSample sample in samples)
            {
                if (text.Length > 0) text.Append(',');
                text.Append(Invariant(sample.Handles));
                text.Append(':');
                text.Append(Invariant(sample.Threads));
            }
            return text.ToString();
        }

        private static string Invariant(int value)
        {
            return value.ToString(System.Globalization.CultureInfo.InvariantCulture);
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
