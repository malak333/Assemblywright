using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;

// GUI process-boundary fixture. It has no network or credential operations.
internal static class GuiGitHubFixture
{
    private static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
    private static string Value(string[] args, string key)
    {
        for (int index = 0; index + 1 < args.Length; index++) if (args[index] == key) return args[index + 1];
        return null;
    }
    private static Dictionary<string, object> Map(params object[] pairs)
    {
        Dictionary<string, object> result = new Dictionary<string, object>();
        for (int index = 0; index < pairs.Length; index += 2) result[(string)pairs[index]] = pairs[index + 1];
        return result;
    }
    private static Dictionary<string, object> Repo(string name)
    {
        return Map("nameWithOwner", name, "url", "https://github.com/" + name,
            "visibility", "PRIVATE", "defaultBranchRef", Map("name", "main"));
    }
    private static string Head(string project)
    {
        string head = File.ReadAllText(Path.Combine(project, ".git", "HEAD")).Trim();
        return head.StartsWith("ref: ") ? File.ReadAllText(Path.Combine(project, ".git", head.Substring(5).Replace('/', Path.DirectorySeparatorChar))).Trim() : head;
    }
    private static string Mode(string state)
    {
        string path = Path.Combine(state, "github-fixture-mode.txt");
        return File.Exists(path) ? File.ReadAllText(path).Trim() : "success";
    }
    private static string ProjectForOutput(string project, string mode)
    {
        if (!mode.StartsWith("verbatim-", StringComparison.Ordinal)) return project;
        return project.StartsWith(@"\\", StringComparison.Ordinal) ? @"\\?\UNC\" + project.Substring(2) : @"\\?\" + project;
    }
    private static Dictionary<string, object> ConnectionForOutput(Dictionary<string, object> connection, string mode)
    {
        if (connection == null) return null;
        Dictionary<string, object> output = new Dictionary<string, object>(connection);
        output["project"] = ProjectForOutput(Convert.ToString(connection["project"]), mode);
        return output;
    }
    private static int Main(string[] args)
    {
        Dictionary<string, object> output = Map("schema_version", 1, "status", "ok");
        try
        {
            string root = Path.GetFullPath(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, ".."));
            string state = Path.GetFullPath(Value(args, "--state-dir") ?? string.Empty);
            if (!state.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("GUI fixture state must stay in its isolated test root.");
            Directory.CreateDirectory(state);
            File.AppendAllText(Path.Combine(state, "github-fixture-calls.jsonl"), Json.Serialize(args) + "\n", new UTF8Encoding(false));
            string storePath = Path.Combine(state, "github-fixture.json");
            Dictionary<string, object> store = File.Exists(storePath)
                ? Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(storePath))
                : Map("revision", 0, "connection", null);
            int revision = Convert.ToInt32(store["revision"]);
            string command = args[0];
            if (command == "account" || command == "login")
            {
                output["status"] = "signed_in"; output["account"] = "gui-owner";
                if (command == "login") Console.Error.WriteLine("Fixture device authorization completed.");
            }
            else if (command == "repositories") output["repositories"] = new object[] { Repo("gui-owner/feature-fixture") };
            else if (command == "create-repository")
            {
                output["status"] = "created"; output["repository"] = Repo(Value(args, "--name"));
                output["message"] = "Fixture repository initialized with a README.";
            }
            else if (command == "create-project")
            {
                string directory = Path.GetFullPath(Value(args, "--directory"));
                if (!directory.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("Fixture project escaped its test root.");
                Directory.CreateDirectory(directory);
                if (Directory.GetFileSystemEntries(directory).Length != 0) throw new InvalidOperationException("The project must be empty.");
                output["status"] = "created"; output["project"] = directory; output["empty"] = true;
                output["message"] = "Empty local project ready. Save the connection separately.";
            }
            else if (command == "connection")
            {
                string project = Path.GetFullPath(Value(args, "--project"));
                if (args[1] != "get")
                {
                    if (Convert.ToInt32(Value(args, "--expected-revision")) != revision) throw new InvalidOperationException("Fixture revision conflict.");
                    store["revision"] = ++revision;
                    store["connection"] = args[1] == "disconnect" ? null : Map("revision", revision, "project", project,
                        "repository", Value(args, "--repository"), "repository_url", "https://github.com/" + Value(args, "--repository") + ".git",
                        "base_branch", Value(args, "--base-branch"), "source_head", Head(project));
                    File.WriteAllText(storePath, Json.Serialize(store), new UTF8Encoding(false));
                }
                Dictionary<string, object> connection = store["connection"] as Dictionary<string, object>;
                output["connection"] = connection != null && Convert.ToString(connection["project"]) == project ? ConnectionForOutput(connection, Mode(state)) : null;
                output["revision"] = revision;
            }
            else if (command == "publish")
            {
                string runId = Value(args, "--run-id");
                Dictionary<string, object> report = Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(Path.Combine(state, "runs", runId, "report.json")));
                if (Convert.ToString(report["status"]) != "completed") throw new InvalidOperationException("The native candidate is incomplete.");
                Dictionary<string, object> connection = (Dictionary<string, object>)store["connection"];
                string mode = Mode(state);
                if (mode == "attention") throw new InvalidOperationException("Fixture required checks failed; publication needs attention.");
                if (mode == "wait")
                {
                    File.WriteAllText(Path.Combine(state, "publication-started.txt"), runId);
                    DateTime until = DateTime.UtcNow.AddSeconds(30);
                    while (DateTime.UtcNow < until)
                    {
                        if (File.Exists(Path.Combine(state, "github", "cancel.flag")) || File.Exists(Path.Combine(state, "pause.flag")))
                            throw new InvalidOperationException("Fixture publication cancelled before merge.");
                        Thread.Sleep(50);
                    }
                }
                output["status"] = "succeeded";
                Dictionary<string, object> publication = Map("run_id", mode == "wrong-run" ? Guid.NewGuid().ToString("D") : runId,
                    "project", ProjectForOutput(Convert.ToString(connection["project"]), mode), "repository", connection["repository"], "base_branch", connection["base_branch"],
                    "connection_revision", connection["revision"], "completion_revision", revision + 1, "source_head", connection["source_head"], "branch", "assemblywright/" + runId,
                    "candidate_commit", report["candidate_commit"], "status", "succeeded", "stage", "complete",
                    "pull_request_number", 7, "pull_request_url", "https://github.com/" + connection["repository"] + "/pull/7",
                    "merge_commit", new string('b', 40), "message", "Merged after fixture required checks.");
                output["publication"] = publication;
                string receiptDirectory = Path.Combine(state, "github", "publications", runId);
                Directory.CreateDirectory(receiptDirectory);
                Dictionary<string, object> receipt = new Dictionary<string, object>(publication);
                receipt["schema_version"] = 1;
                receipt["revision"] = revision + 1;
                receipt["observed_at_ms"] = 1;
                File.WriteAllText(Path.Combine(receiptDirectory, "publication.json"), Json.Serialize(receipt), new UTF8Encoding(false));
                output["revision"] = revision + 1;
                store["revision"] = revision + 1;
                store["publications"] = Map(runId, publication);
                store["publication_receipts"] = Map(runId, Map("verified", true, "revision", revision + 1));
                File.WriteAllText(storePath, Json.Serialize(store), new UTF8Encoding(false));
                if (mode == "verbatim-reconcile")
                {
                    Dictionary<string, object> advanced = new Dictionary<string, object>(connection);
                    advanced["revision"] = revision + 1; advanced["source_head"] = publication["merge_commit"];
                    store["connection"] = advanced;
                    File.WriteAllText(storePath, Json.Serialize(store), new UTF8Encoding(false));
                    throw new InvalidOperationException("Fixture lost the initial response after a confirmed merge.");
                }
            }
            else if (command == "reconcile")
            {
                string runId = Value(args, "--run-id");
                Dictionary<string, object> publications = store.ContainsKey("publications") ? store["publications"] as Dictionary<string, object> : null;
                Dictionary<string, object> publication = publications != null && publications.ContainsKey(runId) ? publications[runId] as Dictionary<string, object> : null;
                if (publication == null) throw new InvalidOperationException("Fixture has no durable publication to reconcile.");
                output["status"] = "succeeded"; output["publication"] = publication; output["revision"] = revision;
            }
            else if (command == "cancel")
            {
                Directory.CreateDirectory(Path.Combine(state, "github"));
                File.WriteAllText(Path.Combine(state, "github", "cancel.flag"), "cancel");
            }
            else if (command == "status")
            {
                output["revision"] = revision; output["connections"] = new Dictionary<string, object>();
                output["publications"] = store.ContainsKey("publications") ? store["publications"] : new Dictionary<string, object>();
                output["publication_receipts"] = store.ContainsKey("publication_receipts") ? store["publication_receipts"] : new Dictionary<string, object>();
                output["paused"] = false; output["busy"] = false;
            }
            else throw new InvalidOperationException("Unsupported GUI fixture command.");
            Console.WriteLine(Json.Serialize(output));
            return 0;
        }
        catch (Exception exception)
        {
            Console.WriteLine(Json.Serialize(Map("schema_version", 1, "status", "error", "message", exception.Message)));
            return 1;
        }
    }
}
