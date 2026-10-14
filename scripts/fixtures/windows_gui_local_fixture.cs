using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;

// GUI-only process fixture. ChatGPT calls are deterministic and credential-free;
// every other command is forwarded to the real native runner in the same bundle.
internal static class GuiLocalFixture
{
    private static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
    private static string Value(string[] args, string key)
    {
        for (int index = 0; index + 1 < args.Length; index++) if (args[index] == key) return args[index + 1];
        return null;
    }
    private static Dictionary<string, object> Map(params object[] pairs)
    {
        Dictionary<string, object> value = new Dictionary<string, object>();
        for (int index = 0; index < pairs.Length; index += 2) value[(string)pairs[index]] = pairs[index + 1];
        return value;
    }
    private static string Quote(string value)
    {
        if (value.Length > 0 && value.IndexOfAny(new char[] { ' ', '\t', '\n', '\v', '"' }) < 0) return value;
        StringBuilder result = new StringBuilder("\""); int slashes = 0;
        foreach (char character in value)
        {
            if (character == '\\') slashes++;
            else if (character == '"') { result.Append('\\', slashes * 2 + 1); result.Append('"'); slashes = 0; }
            else { result.Append('\\', slashes); result.Append(character); slashes = 0; }
        }
        result.Append('\\', slashes * 2); result.Append('"'); return result.ToString();
    }
    private static int Main(string[] args)
    {
        try
        {
            if (args.Length == 0) throw new InvalidOperationException("Missing fixture command.");
            if (args[0] == "chatgpt-models")
            {
                string state = Path.GetFullPath(Value(args, "--state-dir") ?? string.Empty);
                string delayPath = Path.Combine(state, "chatgpt-fixture-models-delay-ms.txt");
                if (File.Exists(delayPath))
                {
                    int delayMilliseconds;
                    string delayText = File.ReadAllText(delayPath).Trim();
                    File.Delete(delayPath);
                    if (!int.TryParse(delayText, out delayMilliseconds) || delayMilliseconds < 1 || delayMilliseconds > 5000) throw new InvalidDataException("Invalid bounded ChatGPT catalog delay.");
                    Thread.Sleep(delayMilliseconds);
                }
                if (File.Exists(Path.Combine(state, "chatgpt-fixture-models-error.txt"))) { Console.Error.WriteLine("Fixture ChatGPT account is unavailable."); return 1; }
                Console.WriteLine(Json.Serialize(Map("status", "ok", "account_type", "chatgpt", "models", new object[] {
                    Map("id", "gpt-plan", "name", "GPT Plan", "reasoning_efforts", new object[] { "high" }, "default_reasoning_effort", "high"),
                    Map("id", "gpt-review", "name", "GPT Review", "reasoning_efforts", new object[] { "xhigh" }, "default_reasoning_effort", "xhigh")
                })));
                return 0;
            }
            if (args[0] == "chatgpt-chat")
            {
                string state = Path.GetFullPath(Value(args, "--state-dir") ?? string.Empty);
                string requestPath = Path.GetFullPath(Value(args, "--request") ?? string.Empty);
                Dictionary<string, object> request = Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(requestPath, new UTF8Encoding(false, true)));
                if (request == null || request.Count != 3 || !request.ContainsKey("model") || !request.ContainsKey("reasoning_effort") || !request.ContainsKey("prompt")) throw new InvalidDataException("Invalid ChatGPT fixture request.");
                Directory.CreateDirectory(state);
                File.AppendAllText(Path.Combine(state, "chatgpt-fixture-calls.jsonl"), Json.Serialize(request) + "\n", new UTF8Encoding(false));
                string prompt = Convert.ToString(request["prompt"]);
                if (prompt.Contains("__WAIT_FOR_CANCEL__")) { File.WriteAllText(Path.Combine(state, "chatgpt-fixture-waiting.txt"), "waiting", new UTF8Encoding(false)); Thread.Sleep(30000); }
                Console.WriteLine(Json.Serialize(Map("status", "completed", "text", "Implement answer() to return the integer 42 and keep the existing acceptance test green.", "provider", "chatgpt", "model", request["model"], "reasoning_effort", request["reasoning_effort"])));
                return 0;
            }
            string realRunner = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "assemblywright-local-real.exe");
            ProcessStartInfo start = new ProcessStartInfo(realRunner, string.Join(" ", args.Select(Quote).ToArray()));
            start.UseShellExecute = false; start.CreateNoWindow = true; start.RedirectStandardOutput = true; start.RedirectStandardError = true;
            Process child = Process.Start(start); string output = child.StandardOutput.ReadToEnd(); string error = child.StandardError.ReadToEnd(); child.WaitForExit();
            Console.Out.Write(output); Console.Error.Write(error); return child.ExitCode;
        }
        catch (Exception exception) { Console.Error.WriteLine(exception.Message); return 1; }
    }
}
