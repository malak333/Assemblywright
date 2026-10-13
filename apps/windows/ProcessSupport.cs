using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;

namespace Assemblywright.Windows
{
    internal static class SharedFile
    {
        internal const int MaximumJsonBytes = 4 * 1024 * 1024;

        internal static FileStream OpenRead(string path)
        {
            return new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        }

        internal static string ReadUtf8Json(string path)
        {
            using (FileStream stream = OpenRead(path))
            {
                if (stream.Length > MaximumJsonBytes) throw new InvalidDataException("The JSON document exceeds the 4 MiB limit.");
                byte[] bytes = new byte[(int)stream.Length];
                int total = 0;
                while (total < bytes.Length)
                {
                    int count = stream.Read(bytes, total, bytes.Length - total);
                    if (count == 0) break;
                    total += count;
                }
                if (total != bytes.Length || stream.ReadByte() != -1) throw new InvalidDataException("The JSON document changed while it was being read.");
                int offset = bytes.Length >= 3 && bytes[0] == 0xef && bytes[1] == 0xbb && bytes[2] == 0xbf ? 3 : 0;
                return new UTF8Encoding(false, true).GetString(bytes, offset, bytes.Length - offset);
            }
        }
    }

    internal static class WindowsCommandLine
    {
        internal static string Quote(string argument)
        {
            if (argument == null) throw new ArgumentNullException("argument");
            if (argument.Length > 0 && argument.IndexOfAny(new char[] { ' ', '\t', '\n', '\v', '"' }) < 0) return argument;
            StringBuilder result = new StringBuilder();
            result.Append('"');
            int slashes = 0;
            foreach (char character in argument)
            {
                if (character == '\\')
                {
                    slashes++;
                }
                else if (character == '"')
                {
                    result.Append('\\', slashes * 2 + 1);
                    result.Append('"');
                    slashes = 0;
                }
                else
                {
                    result.Append('\\', slashes);
                    slashes = 0;
                    result.Append(character);
                }
            }
            result.Append('\\', slashes * 2);
            result.Append('"');
            return result.ToString();
        }

        internal static string Join(IEnumerable<string> arguments)
        {
            StringBuilder result = new StringBuilder();
            foreach (string argument in arguments)
            {
                if (result.Length > 0) result.Append(' ');
                result.Append(Quote(argument));
            }
            return result.ToString();
        }
    }

    internal sealed class ProcessResult
    {
        internal int ExitCode;
        internal string StandardOutput;
        internal string StandardError;
    }

    internal static class ProcessRunner
    {
        internal sealed class ProcessLaunchException : Exception
        {
            internal readonly Process ActiveProcess;

            internal ProcessLaunchException(string message, Exception innerException, Process activeProcess)
                : base(message, innerException)
            {
                ActiveProcess = activeProcess;
            }
        }

        internal static Process StartStreaming(string executable, IEnumerable<string> arguments, Action<string> output, Action<string> error)
        {
            ProcessStartInfo start = NewStartInfo(executable, arguments);
            Process process = new Process();
            bool started = false;
            process.StartInfo = start;
            process.OutputDataReceived += delegate(object sender, DataReceivedEventArgs eventArgs)
            {
                if (eventArgs.Data != null && output != null) output(eventArgs.Data + Environment.NewLine);
            };
            process.ErrorDataReceived += delegate(object sender, DataReceivedEventArgs eventArgs)
            {
                if (eventArgs.Data != null && error != null) error(eventArgs.Data + Environment.NewLine);
            };
            try
            {
                if (!process.Start()) throw new InvalidOperationException("Windows did not start " + executable + ".");
                started = true;
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();
                return process;
            }
            catch (Exception exception)
            {
                if (!started)
                {
                    process.Dispose();
                    throw;
                }
                if (TryTerminateAndDispose(process, 5000)) throw;
                throw new ProcessLaunchException("The child process started, but stream setup failed and its exit could not be confirmed.", exception, process);
            }
        }

        internal static bool TryTerminateAndDispose(Process process, int timeoutMilliseconds)
        {
            if (process == null) return true;
            bool exited = false;
            try
            {
                if (!process.HasExited)
                {
                    try { process.Kill(); }
                    catch { }
                }
                exited = process.HasExited || process.WaitForExit(timeoutMilliseconds);
            }
            catch
            {
                exited = false;
            }
            if (!exited) return false;
            try { process.WaitForExit(); }
            catch { return false; }
            process.Dispose();
            return true;
        }

        internal static Task<ProcessResult> RunCapturedAsync(string executable, IEnumerable<string> arguments)
        {
            TaskCompletionSource<ProcessResult> completion = new TaskCompletionSource<ProcessResult>();
            StringBuilder output = new StringBuilder();
            StringBuilder error = new StringBuilder();
            Process process = new Process();
            bool started = false;
            TaskCompletionSource<bool> exited = new TaskCompletionSource<bool>();
            process.StartInfo = NewStartInfo(executable, arguments);
            process.OutputDataReceived += delegate(object sender, DataReceivedEventArgs eventArgs)
            {
                if (eventArgs.Data != null) lock (output) output.AppendLine(eventArgs.Data);
            };
            process.ErrorDataReceived += delegate(object sender, DataReceivedEventArgs eventArgs)
            {
                if (eventArgs.Data != null) lock (error) error.AppendLine(eventArgs.Data);
            };
            process.Exited += delegate { exited.TrySetResult(true); };
            exited.Task.ContinueWith(delegate(Task<bool> signal)
            {
                if (signal.Status != TaskStatus.RanToCompletion) return;
                ProcessResult result = null;
                Exception failure = null;
                try
                {
                    process.WaitForExit();
                    result = new ProcessResult();
                    result.ExitCode = process.ExitCode;
                    lock (output) result.StandardOutput = output.ToString();
                    lock (error) result.StandardError = error.ToString();
                }
                catch (Exception exception)
                {
                    failure = exception;
                }
                try
                {
                    process.Dispose();
                }
                catch (Exception exception)
                {
                    if (failure == null) failure = exception;
                }
                if (failure == null) completion.TrySetResult(result);
                else completion.TrySetException(failure);
            }, TaskScheduler.Default);
            try
            {
                if (!process.Start()) throw new InvalidOperationException("Windows did not start " + executable + ".");
                started = true;
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();
                process.EnableRaisingEvents = true;
            }
            catch (Exception exception)
            {
                exited.TrySetCanceled();
                Exception reported = exception;
                if (!started)
                {
                    process.Dispose();
                }
                else if (!TryTerminateAndDispose(process, 5000))
                {
                    reported = new ProcessLaunchException("The child process started, but stream setup failed and its exit could not be confirmed.", exception, process);
                }
                completion.TrySetException(reported);
                return completion.Task;
            }
            return completion.Task;
        }

        private static ProcessStartInfo NewStartInfo(string executable, IEnumerable<string> arguments)
        {
            ProcessStartInfo start = new ProcessStartInfo();
            start.FileName = executable;
            start.Arguments = WindowsCommandLine.Join(arguments);
            start.UseShellExecute = false;
            start.CreateNoWindow = true;
            start.RedirectStandardOutput = true;
            start.RedirectStandardError = true;
            start.StandardOutputEncoding = Encoding.UTF8;
            start.StandardErrorEncoding = Encoding.UTF8;
            start.WorkingDirectory = AppDomain.CurrentDomain.BaseDirectory;
            return start;
        }
    }

    internal sealed class BoundedLog
    {
        private readonly int maximumBytes;
        private string text;

        internal BoundedLog(int maximumBytes)
        {
            this.maximumBytes = maximumBytes;
            text = string.Empty;
        }

        internal string Append(string value)
        {
            text += value ?? string.Empty;
            Encoding utf8 = new UTF8Encoding(false, false);
            while (utf8.GetByteCount(text) > maximumBytes && text.Length > 0)
            {
                int remove = Math.Max(1, text.Length / 8);
                if (remove < text.Length && char.IsLowSurrogate(text[remove]) && char.IsHighSurrogate(text[remove - 1])) remove++;
                text = text.Substring(Math.Min(remove, text.Length));
            }
            return text;
        }

        internal int ByteCount
        {
            get { return Encoding.UTF8.GetByteCount(text); }
        }
    }

    internal sealed class BoundedCapture
    {
        private readonly int maximumBytes;
        private readonly StringBuilder text;
        private bool overflowed;

        internal BoundedCapture(int maximumBytes)
        {
            this.maximumBytes = maximumBytes;
            text = new StringBuilder();
        }

        internal void Append(string value)
        {
            if (value == null) return;
            lock (text)
            {
                if (overflowed) return;
                string candidate = text.ToString() + value;
                if (Encoding.UTF8.GetByteCount(candidate) > maximumBytes)
                {
                    overflowed = true;
                    text.Clear();
                    return;
                }
                text.Append(value);
            }
        }

        internal bool Overflowed
        {
            get { lock (text) return overflowed; }
        }

        internal string Text
        {
            get { lock (text) return text.ToString(); }
        }
    }
}
