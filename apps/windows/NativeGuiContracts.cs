using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Threading;
using System.Threading.Tasks;

namespace Assemblywright.Windows
{
    internal sealed class PermissionsDocument
    {
        public int schema_version { get; set; }
        public string mode { get; set; }
        public long revision { get; set; }
        public string execution_host { get; set; }
        public bool available { get; set; }
        public PermissionsDocument() { }
    }

    internal sealed class ApprovalDocument
    {
        public int schema_version { get; set; }
        public string status { get; set; }
        public string id { get; set; }
        public string token { get; set; }
        public long revision { get; set; }
        public string run_id { get; set; }
        public string action { get; set; }
        public string digest { get; set; }
        public string detail { get; set; }
        public object requested_at_ms { get; set; }
        public object decided_at_ms { get; set; }
        public object approved { get; set; }
        public ApprovalDocument() { }
    }

    internal static class NativeGuiControl
    {
        internal static bool IsValidPermissions(PermissionsDocument value)
        {
            return value != null && value.schema_version == 1 && value.execution_host == "windows" && value.revision >= 0 && (value.mode == "ask" || value.mode == "auto" || value.mode == "full");
        }

        internal static bool AcknowledgesPermissionChange(PermissionsDocument value, string requestedMode, long expectedRevision)
        {
            return IsValidPermissions(value) && value.mode == requestedMode && value.revision == expectedRevision + 1;
        }

        internal static bool AcknowledgesApproval(ApprovalDocument value, ApprovalDocument expected, bool approve)
        {
            string requested = expected == null ? string.Empty : Convert.ToString(expected.requested_at_ms, System.Globalization.CultureInfo.InvariantCulture);
            return value != null && expected != null && value.id == expected.id && value.token == expected.token && value.revision == expected.revision && value.run_id == expected.run_id && value.action == expected.action && value.digest == expected.digest
                && value.detail == expected.detail && Convert.ToString(value.requested_at_ms, System.Globalization.CultureInfo.InvariantCulture) == requested
                && value.status == (approve ? "approved" : "rejected") && value.decided_at_ms != null && value.approved is bool && (bool)value.approved == approve;
        }

        internal static bool IsValidPendingApproval(ApprovalDocument value)
        {
            return value != null && value.status == "pending" && value.schema_version == 1 && !string.IsNullOrWhiteSpace(value.id) && !string.IsNullOrWhiteSpace(value.token)
                && value.revision >= 1 && !string.IsNullOrWhiteSpace(value.run_id) && (value.action == "apply_model_edits" || value.action == "run_validation")
                && !string.IsNullOrWhiteSpace(value.digest) && value.requested_at_ms != null && value.decided_at_ms == null && value.approved == null;
        }

        internal static async Task<PermissionsDocument> GetPermissionsAsync(string runnerPath, string stateDirectory)
        {
            ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "permissions", "get", "--state-dir", stateDirectory });
            if (result.ExitCode != 0) throw new InvalidOperationException(Error(result, "Permissions are unavailable."));
            PermissionsDocument value = Serializer.New().Deserialize<PermissionsDocument>(result.StandardOutput);
            ValidatePermissions(value); return value;
        }

        internal static async Task<PermissionsDocument> SetPermissionsAsync(string runnerPath, string stateDirectory, string mode, long expectedRevision)
        {
            ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "permissions", "set", "--state-dir", stateDirectory, "--mode", mode, "--expected-revision", expectedRevision.ToString(System.Globalization.CultureInfo.InvariantCulture) });
            if (result.ExitCode != 0) throw new InvalidOperationException(Error(result, "Permissions could not be saved."));
            PermissionsDocument value = Serializer.New().Deserialize<PermissionsDocument>(result.StandardOutput);
            ValidatePermissions(value);
            if (!AcknowledgesPermissionChange(value, mode, expectedRevision)) throw new InvalidDataException("The native permission acknowledgement did not match the owner request.");
            return value;
        }

        internal static async Task<ApprovalDocument> GetApprovalAsync(string runnerPath, string stateDirectory)
        {
            ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "approval", "show", "--state-dir", stateDirectory });
            if (result.ExitCode != 0) throw new InvalidOperationException(Error(result, "Approval state is unavailable."));
            ApprovalDocument value = Serializer.New().Deserialize<ApprovalDocument>(result.StandardOutput);
            if (value == null || (value.status != "idle" && value.status != "pending")) throw new InvalidDataException("The native approval response is invalid.");
            if (value.status == "pending" && !IsValidPendingApproval(value)) throw new InvalidDataException("The pending approval is incomplete.");
            return value;
        }

        internal static async Task<ApprovalDocument> DecideApprovalAsync(string runnerPath, string stateDirectory, ApprovalDocument expected, bool approve)
        {
            string requested = Convert.ToString(expected.requested_at_ms, System.Globalization.CultureInfo.InvariantCulture);
            ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "approval", "decide", "--state-dir", stateDirectory, "--id", expected.id, "--token", expected.token, "--revision", expected.revision.ToString(System.Globalization.CultureInfo.InvariantCulture), "--run-id", expected.run_id, "--action", expected.action, "--digest", expected.digest, "--requested-at-ms", requested, "--approve", approve ? "true" : "false" });
            if (result.ExitCode != 0) throw new InvalidOperationException(Error(result, "The approval decision was not accepted."));
            ApprovalDocument value = Serializer.New().Deserialize<ApprovalDocument>(result.StandardOutput);
            if (!AcknowledgesApproval(value, expected, approve)) throw new InvalidDataException("The native approval acknowledgement did not match the exact displayed request and owner decision.");
            return value;
        }

        private static void ValidatePermissions(PermissionsDocument value)
        {
            if (!IsValidPermissions(value)) throw new InvalidDataException("The native permissions response is invalid.");
        }

        private static string Error(ProcessResult result, string fallback)
        {
            string text = string.IsNullOrWhiteSpace(result.StandardError) ? result.StandardOutput : result.StandardError;
            return string.IsNullOrWhiteSpace(text) ? fallback : text.Trim();
        }
    }

    internal sealed class ModelCatalogResponse
    {
        public List<ModelCatalogItem> data { get; set; }
        public ModelCatalogResponse() { data = new List<ModelCatalogItem>(); }
    }

    internal sealed class ModelCatalogItem
    {
        public string id { get; set; }
        public List<string> reasoning_efforts { get; set; }
        public ModelCapabilities capabilities { get; set; }
        public ModelCatalogItem() { }
    }

    internal sealed class ModelCapabilities { public List<string> reasoning_efforts { get; set; } public ModelCapabilities() { } }
    internal sealed class LocalModelOption
    {
        internal string Id;
        internal List<string> ReasoningEfforts;
        internal LocalModelOption(string id, IEnumerable<string> efforts) { Id = id; ReasoningEfforts = (efforts ?? new string[0]).Where(ReasoningEffort.IsValid).Where(delegate(string value) { return !string.IsNullOrWhiteSpace(value); }).Distinct(StringComparer.Ordinal).ToList(); }
        internal IEnumerable<string> AvailableEfforts { get { return ReasoningEfforts.Count == 0 ? (IEnumerable<string>)ReasoningEffort.Values : ReasoningEfforts; } }
    }

    internal static class LocalModelCatalog
    {
        internal static async Task<List<LocalModelOption>> LoadAsync(string endpoint, CancellationToken cancellationToken)
        {
            string error = LoopbackEndpoint.Validate(endpoint); if (error != null) throw new InvalidDataException(error);
            HttpWebRequest request = (HttpWebRequest)WebRequest.Create(endpoint.TrimEnd('/') + "/models");
            request.Method = "GET"; request.Proxy = null; request.AllowAutoRedirect = false; request.Timeout = 10000; request.ReadWriteTimeout = 10000;
            using (CancellationTokenSource deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
            {
                deadline.CancelAfter(10000);
                using (deadline.Token.Register(delegate { request.Abort(); }))
                {
                    WebResponse response;
                    try { response = await Task.Factory.FromAsync<WebResponse>(request.BeginGetResponse, request.EndGetResponse, null); }
                    catch (WebException exception) { if (deadline.IsCancellationRequested) throw new OperationCanceledException(deadline.Token); throw new InvalidOperationException("Could not load local models: " + exception.Message); }
                    using (response)
                    using (Stream stream = response.GetResponseStream())
                    {
                        string json = await ReadBoundedAsync(stream, deadline.Token);
                        ModelCatalogResponse catalog = Serializer.New().Deserialize<ModelCatalogResponse>(json);
                        if (catalog == null || catalog.data == null) throw new InvalidDataException("The local model catalog response is invalid.");
                        List<LocalModelOption> values = catalog.data.Where(delegate(ModelCatalogItem item) { return item != null && !string.IsNullOrWhiteSpace(item.id) && item.id.Length <= 200; }).GroupBy(delegate(ModelCatalogItem item) { return item.id; }, StringComparer.Ordinal).Select(delegate(IGrouping<string, ModelCatalogItem> group) { ModelCatalogItem item = group.First(); IEnumerable<string> efforts = item.reasoning_efforts ?? (item.capabilities == null ? null : item.capabilities.reasoning_efforts); return new LocalModelOption(item.id, efforts); }).OrderBy(delegate(LocalModelOption item) { return item.Id; }, StringComparer.Ordinal).ToList();
                        if (values.Count == 0) throw new InvalidDataException("The local model catalog did not contain a usable model.");
                        return values;
                    }
                }
            }
        }

        private static Task<string> ReadBoundedAsync(Stream stream, CancellationToken cancellationToken)
        {
            return Task.Run(delegate
            {
                const int maximum = 2 * 1024 * 1024;
                byte[] buffer = new byte[8192];
                using (MemoryStream output = new MemoryStream())
                {
                    while (true)
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        int count = stream.Read(buffer, 0, buffer.Length);
                        if (count == 0) break;
                        if (output.Length + count > maximum) throw new InvalidDataException("The local model catalog exceeded 2 MiB.");
                        output.Write(buffer, 0, count);
                    }
                    return new System.Text.UTF8Encoding(false, true).GetString(output.ToArray());
                }
            }, cancellationToken);
        }
    }

    internal sealed class ChatGptModelOption
    {
        internal string Id;
        internal string Name;
        internal List<string> ReasoningEfforts;
        internal string DefaultReasoningEffort;

        internal ChatGptModelOption(string id, string name, IEnumerable<string> efforts, string defaultEffort)
        {
            Id = id;
            Name = name;
            List<string> supplied = (efforts ?? new string[0]).ToList();
            if (supplied.Any(delegate(string value) { return string.IsNullOrWhiteSpace(value) || !ReasoningEffort.IsValid(value); }) || supplied.Distinct(StringComparer.Ordinal).Count() != supplied.Count)
                throw new InvalidDataException("The ChatGPT model catalog contains invalid reasoning choices.");
            ReasoningEfforts = supplied;
            DefaultReasoningEffort = defaultEffort;
            if (string.IsNullOrWhiteSpace(Id) || Id != Id.Trim() || Id.Length > 200 || string.IsNullOrWhiteSpace(Name) || Name != Name.Trim() || Name.Length > 200 || ReasoningEfforts.Count == 0
                || !ReasoningEfforts.Contains(DefaultReasoningEffort)) throw new InvalidDataException("The ChatGPT model catalog contains an invalid model.");
        }
    }

    internal static class ChatGptModelCatalog
    {
        internal const int MaximumBytes = 2 * 1024 * 1024;

        internal static async Task<List<ChatGptModelOption>> LoadAsync(string runnerPath, string stateDirectory)
        {
            ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "chatgpt-models", "--state-dir", stateDirectory });
            string output = result.StandardOutput ?? string.Empty;
            if (System.Text.Encoding.UTF8.GetByteCount(output) > MaximumBytes) throw new InvalidDataException("The ChatGPT model catalog exceeded 2 MiB.");
            if (result.ExitCode != 0) throw new InvalidOperationException(FirstError(result, "ChatGPT models are unavailable."));
            object parsed = Serializer.New().DeserializeObject(output);
            Dictionary<string, object> root = parsed as Dictionary<string, object>;
            RequireKeys(root, new string[] { "status", "account_type", "models" }, "The ChatGPT model catalog response is invalid.");
            if (!string.Equals(Convert.ToString(root["status"]), "ok", StringComparison.Ordinal)
                || !string.Equals(Convert.ToString(root["account_type"]), "chatgpt", StringComparison.Ordinal)) throw new InvalidDataException("The ChatGPT account is not ready.");
            object[] rows = root["models"] as object[];
            if (rows == null || rows.Length == 0 || rows.Length > 100) throw new InvalidDataException("The ChatGPT model catalog did not contain a usable model.");
            List<ChatGptModelOption> models = new List<ChatGptModelOption>();
            HashSet<string> ids = new HashSet<string>(StringComparer.Ordinal);
            foreach (object rowValue in rows)
            {
                Dictionary<string, object> row = rowValue as Dictionary<string, object>;
                RequireKeys(row, new string[] { "id", "name", "reasoning_efforts", "default_reasoning_effort" }, "The ChatGPT model catalog contains an invalid row.");
                object[] effortValues = row["reasoning_efforts"] as object[];
                if (effortValues == null) throw new InvalidDataException("The ChatGPT model catalog contains invalid reasoning choices.");
                List<string> efforts = effortValues.Select(Convert.ToString).ToList();
                ChatGptModelOption option = new ChatGptModelOption(Convert.ToString(row["id"]), Convert.ToString(row["name"]), efforts, Convert.ToString(row["default_reasoning_effort"]));
                if (!ids.Add(option.Id)) throw new InvalidDataException("The ChatGPT model catalog contains duplicate model IDs.");
                models.Add(option);
            }
            return models;
        }

        internal static void RequireKeys(Dictionary<string, object> value, IEnumerable<string> expected, string message)
        {
            if (value == null) throw new InvalidDataException(message);
            HashSet<string> keys = new HashSet<string>(expected, StringComparer.Ordinal);
            if (value.Count != keys.Count || value.Keys.Any(delegate(string key) { return !keys.Contains(key); })) throw new InvalidDataException(message);
        }

        private static string FirstError(ProcessResult result, string fallback)
        {
            string text = string.IsNullOrWhiteSpace(result.StandardError) ? result.StandardOutput : result.StandardError;
            return string.IsNullOrWhiteSpace(text) ? fallback : text.Trim();
        }
    }
}
