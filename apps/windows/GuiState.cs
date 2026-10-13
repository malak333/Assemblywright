using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Web.Script.Serialization;

namespace Assemblywright.Windows
{
    internal sealed class StartOperationGate
    {
        private int active;

        internal bool TryEnter()
        {
            return System.Threading.Interlocked.CompareExchange(ref active, 1, 0) == 0;
        }

        internal void Exit()
        {
            System.Threading.Interlocked.Exchange(ref active, 0);
        }

        internal bool IsActive
        {
            get { return System.Threading.Interlocked.CompareExchange(ref active, 0, 0) != 0; }
        }
    }

    internal static class PrelaunchAuthority
    {
        internal static bool MayLaunch(bool stopRequested, bool closeRequested)
        {
            return !stopRequested && !closeRequested;
        }
    }

    internal sealed class GuiSettings
    {
        public int schema_version { get; set; }
        public string endpoint { get; set; }
        public string model { get; set; }
        public string orchestrator_provider { get; set; }
        public string orchestrator_model { get; set; }
        public string orchestrator_reasoning_effort { get; set; }
        public string reviewer_provider { get; set; }
        public string reviewer_model { get; set; }
        public string reasoning_effort { get; set; }
        public string reviewer_reasoning_effort { get; set; }
        public long revision { get; set; }
        public int default_attempts { get; set; }
        public int timeout_seconds { get; set; }

        public GuiSettings()
        {
            schema_version = 1;
            endpoint = "http://127.0.0.1:8080/v1";
            model = "qwen3.6-35b-a3b";
            orchestrator_provider = RoleProvider.ChatGpt;
            orchestrator_model = string.Empty;
            orchestrator_reasoning_effort = string.Empty;
            reviewer_provider = RoleProvider.ChatGpt;
            reviewer_model = string.Empty;
            reasoning_effort = "none";
            reviewer_reasoning_effort = string.Empty;
            revision = 0;
            default_attempts = 3;
            timeout_seconds = 300;
        }
    }

    internal static class GuiSettingsStore
    {
        internal static GuiSettings Load(string path)
        {
            if (!File.Exists(path)) return new GuiSettings();
            string json = File.ReadAllText(path, new UTF8Encoding(false, true));
            JavaScriptSerializer serializer = Serializer.New();
            Dictionary<string, object> raw = serializer.DeserializeObject(json) as Dictionary<string, object>;
            GuiSettings settings = serializer.Deserialize<GuiSettings>(json);
            bool migratingLegacyRoles = raw != null && (!raw.ContainsKey("orchestrator_provider") || !raw.ContainsKey("reviewer_provider"));
            ApplyBackwardCompatibleDefaults(settings, raw);
            string error = Validate(settings, migratingLegacyRoles);
            if (error != null) throw new InvalidDataException("Invalid GUI settings: " + error);
            return settings;
        }

        private static void ApplyBackwardCompatibleDefaults(GuiSettings settings, Dictionary<string, object> raw)
        {
            if (settings == null) return;
            bool hasOrchestratorProvider = raw != null && raw.ContainsKey("orchestrator_provider");
            bool hasReviewerProvider = raw != null && raw.ContainsKey("reviewer_provider");
            object legacyModelValue;
            string legacyModel = raw != null && raw.TryGetValue("model", out legacyModelValue) ? Convert.ToString(legacyModelValue) : settings.model;
            if (string.IsNullOrWhiteSpace(settings.reasoning_effort)) settings.reasoning_effort = "none";
            if (string.IsNullOrWhiteSpace(settings.reviewer_reasoning_effort)) settings.reviewer_reasoning_effort = "none";
            if (!hasOrchestratorProvider || string.IsNullOrWhiteSpace(settings.orchestrator_provider))
            {
                settings.orchestrator_provider = RoleProvider.ChatGpt;
                settings.orchestrator_model = string.Empty;
                settings.orchestrator_reasoning_effort = string.Empty;
            }
            if (!hasReviewerProvider || string.IsNullOrWhiteSpace(settings.reviewer_provider))
            {
                settings.reviewer_provider = RoleProvider.ChatGpt;
                settings.reviewer_model = string.Empty;
                settings.reviewer_reasoning_effort = string.Empty;
            }
            else if (string.IsNullOrWhiteSpace(settings.reviewer_model) && settings.reviewer_provider == RoleProvider.Local) settings.reviewer_model = legacyModel;
        }

        internal static void Save(string path, GuiSettings settings)
        {
            string error = Validate(settings);
            if (error != null) throw new InvalidDataException(error);
            JobJson.WriteUtf8(path, Serializer.New().Serialize(settings));
        }

        internal static string Validate(GuiSettings settings)
        {
            return Validate(settings, false);
        }

        private static string Validate(GuiSettings settings, bool allowUnconfiguredChatGpt)
        {
            if (settings == null || settings.schema_version != 1) return "Settings schema must be 1.";
            string endpointError = LoopbackEndpoint.Validate(settings.endpoint);
            if (endpointError != null) return endpointError;
            if (string.IsNullOrWhiteSpace(settings.model) || settings.model.Length > 200) return "Enter a local model name of at most 200 characters.";
            if (!RoleProvider.IsValid(settings.orchestrator_provider) || !RoleProvider.IsValid(settings.reviewer_provider)) return "Choose local AI or ChatGPT for each role.";
            if (settings.revision < 0) return "Settings revision cannot be negative.";
            bool unconfiguredOrchestrator = allowUnconfiguredChatGpt && settings.orchestrator_provider == RoleProvider.ChatGpt && string.IsNullOrWhiteSpace(settings.orchestrator_model) && string.IsNullOrWhiteSpace(settings.orchestrator_reasoning_effort);
            bool unconfiguredReviewer = allowUnconfiguredChatGpt && settings.reviewer_provider == RoleProvider.ChatGpt && string.IsNullOrWhiteSpace(settings.reviewer_model) && string.IsNullOrWhiteSpace(settings.reviewer_reasoning_effort);
            if (!unconfiguredOrchestrator && (string.IsNullOrWhiteSpace(settings.orchestrator_model) || settings.orchestrator_model.Length > 200)) return "Choose an orchestrator model of at most 200 characters.";
            if (!unconfiguredReviewer && (string.IsNullOrWhiteSpace(settings.reviewer_model) || settings.reviewer_model.Length > 200)) return "Enter a reviewer model name of at most 200 characters.";
            if (!ReasoningEffort.IsValid(settings.reasoning_effort) || (!unconfiguredOrchestrator && (!ReasoningEffort.IsValid(settings.orchestrator_reasoning_effort) || string.IsNullOrWhiteSpace(settings.orchestrator_reasoning_effort)))
                || (!unconfiguredReviewer && (!ReasoningEffort.IsValid(settings.reviewer_reasoning_effort) || string.IsNullOrWhiteSpace(settings.reviewer_reasoning_effort)))) return "Choose a supported reasoning level.";
            if (settings.default_attempts < 1 || settings.default_attempts > 20) return "Attempts must be between 1 and 20.";
            if (settings.timeout_seconds < 1 || settings.timeout_seconds > 3600) return "Timeout must be between 1 and 3600 seconds.";
            return null;
        }
    }

    internal static class LoopbackEndpoint
    {
        internal static string Validate(string value)
        {
            Uri endpoint;
            if (!Uri.TryCreate(value, UriKind.Absolute, out endpoint) || endpoint.Scheme != Uri.UriSchemeHttp)
                return "The endpoint must be an HTTP loopback URL ending in /v1.";
            bool loopback = endpoint.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase)
                || endpoint.Host == "127.0.0.1" || endpoint.Host == "::1";
            if (!loopback || endpoint.AbsolutePath.TrimEnd('/') != "/v1" || !string.IsNullOrEmpty(endpoint.Query)
                || !string.IsNullOrEmpty(endpoint.Fragment) || !string.IsNullOrEmpty(endpoint.UserInfo))
                return "The endpoint must be an HTTP loopback URL ending in /v1.";
            return null;
        }

        internal static Uri ChatCompletions(string value)
        {
            string error = Validate(value);
            if (error != null) throw new InvalidDataException(error);
            return new Uri(value.TrimEnd('/') + "/chat/completions", UriKind.Absolute);
        }
    }

    internal static class RoleSettings
    {
        internal static string ReviewerModelForJob(JobDocument job, GuiSettings settings)
        {
            if (job != null && !string.IsNullOrWhiteSpace(job.reviewer_model)) return job.reviewer_model;
            if (job != null && !string.IsNullOrWhiteSpace(job.model)) return job.model;
            return settings == null ? string.Empty : settings.reviewer_model;
        }

        internal static string ReviewerProviderForJob(JobDocument job)
        {
            return job != null && RoleProvider.IsValid(job.reviewer_provider) ? job.reviewer_provider : RoleProvider.Local;
        }
    }

    internal sealed class AssemblyLineDocument
    {
        public int schema_version { get; set; }
        public List<AssemblyLineEntry> entries { get; set; }

        public AssemblyLineDocument()
        {
            schema_version = 1;
            entries = new List<AssemblyLineEntry>();
        }
    }

    internal sealed class AssemblyLineEntry
    {
        public string id { get; set; }
        public string title { get; set; }
        public string repository { get; set; }
        public string frozen_job_json { get; set; }
        public string status { get; set; }
        public string stage { get; set; }
        public int attempt { get; set; }
        public string result { get; set; }
        public string run_id { get; set; }
        public string publication_status { get; set; }
        public string publication_url { get; set; }
        public string publication_merge_sha { get; set; }
        public long created_at_ms { get; set; }
        public long updated_at_ms { get; set; }

        public AssemblyLineEntry() { }
    }

    internal static class AssemblyLineStore
    {
        internal const int MaximumPending = 100;

        internal static AssemblyLineDocument Load(string path)
        {
            if (!File.Exists(path)) return new AssemblyLineDocument();
            string json = File.ReadAllText(path, new UTF8Encoding(false, true));
            AssemblyLineDocument document = Serializer.New().Deserialize<AssemblyLineDocument>(json);
            Validate(document);
            return document;
        }

        internal static void Save(string path, AssemblyLineDocument document)
        {
            Validate(document);
            JobJson.WriteUtf8(path, Serializer.New().Serialize(document));
        }

        internal static int RecoverInterrupted(AssemblyLineDocument document)
        {
            int recovered = 0;
            foreach (AssemblyLineEntry entry in document.entries)
            {
                if (entry.status == "Running")
                {
                    entry.status = "Interrupted";
                    entry.result = "Assemblywright closed before this owned run reached a confirmed terminal report. Start again only after owner review.";
                    entry.updated_at_ms = NowMilliseconds();
                    recovered++;
                }
            }
            return recovered;
        }

        internal static AssemblyLineEntry Add(AssemblyLineDocument document, JobDocument job)
        {
            int pending = document.entries.Count(delegate(AssemblyLineEntry entry) { return entry.status == "Pending" || entry.status == "Running"; });
            if (pending >= MaximumPending) throw new InvalidOperationException("The Assembly Line already has 100 pending or running features.");
            string frozen = JobJson.Serialize(job);
            JobDocument roundTrip = JobJson.DeserializeJob(frozen);
            string error = JobValidator.ValidateForForm(roundTrip, true);
            if (error != null) throw new InvalidDataException(error);
            long now = NowMilliseconds();
            AssemblyLineEntry created = new AssemblyLineEntry();
            created.id = Guid.NewGuid().ToString("D");
            created.title = TitleFromSpecification(job.specification);
            created.repository = Path.GetFullPath(job.repository);
            created.frozen_job_json = frozen;
            created.status = "Pending";
            created.stage = "queued";
            created.attempt = 0;
            created.result = "Waiting for the owner to start the Assembly Line.";
            created.run_id = string.Empty;
            created.publication_status = "local_only";
            created.publication_url = string.Empty;
            created.publication_merge_sha = string.Empty;
            created.created_at_ms = now;
            created.updated_at_ms = now;
            document.entries.Add(created);
            return created;
        }

        internal static AssemblyLineEntry FirstPending(AssemblyLineDocument document)
        {
            return document.entries.FirstOrDefault(delegate(AssemblyLineEntry entry) { return entry.status == "Pending"; });
        }

        internal static bool Retry(AssemblyLineEntry entry)
        {
            if (entry == null || (entry.status != "Failed" && entry.status != "Cancelled" && entry.status != "Interrupted")) return false;
            entry.status = "Pending";
            entry.stage = "queued";
            entry.attempt = 0;
            entry.result = "Retry approved by the owner. Waiting for Start line.";
            entry.run_id = string.Empty;
            entry.updated_at_ms = NowMilliseconds();
            return true;
        }

        internal static bool Remove(AssemblyLineDocument document, AssemblyLineEntry entry)
        {
            if (entry == null || entry.status == "Running") return false;
            return document.entries.Remove(entry);
        }

        private static void Validate(AssemblyLineDocument document)
        {
            if (document == null || document.schema_version != 1 || document.entries == null)
                throw new InvalidDataException("Assembly Line state must use schema 1 with an entries array.");
            HashSet<string> ids = new HashSet<string>(StringComparer.Ordinal);
            int pending = 0;
            int running = 0;
            foreach (AssemblyLineEntry entry in document.entries)
            {
                Guid ignored;
                if (entry == null || !Guid.TryParseExact(entry.id, "D", out ignored) || !ids.Add(entry.id))
                    throw new InvalidDataException("Assembly Line entries require unique canonical IDs.");
                if (string.IsNullOrWhiteSpace(entry.title) || string.IsNullOrWhiteSpace(entry.repository)
                    || string.IsNullOrWhiteSpace(entry.frozen_job_json))
                    throw new InvalidDataException("Assembly Line entries are incomplete.");
                if (entry.status != "Pending" && entry.status != "Running" && entry.status != "Completed"
                    && entry.status != "Failed" && entry.status != "Cancelled" && entry.status != "Interrupted")
                    throw new InvalidDataException("Assembly Line entry has an unknown status.");
                JobDocument job = JobJson.DeserializeJob(entry.frozen_job_json);
                string error = JobValidator.ValidateForForm(job, false);
                if (error != null) throw new InvalidDataException("Assembly Line frozen job is invalid: " + error);
                if (!string.Equals(Path.GetFullPath(job.repository), Path.GetFullPath(entry.repository), StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("Assembly Line repository does not match its frozen job.");
                if (entry.status == "Pending" || entry.status == "Running") pending++;
                if (entry.status == "Running") running++;
            }
            if (pending > MaximumPending) throw new InvalidDataException("Assembly Line state exceeds the 100-entry pending limit.");
            if (running > 1) throw new InvalidDataException("Assembly Line state contains more than one running feature.");
        }

        internal static long NowMilliseconds()
        {
            return (long)(DateTime.UtcNow - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalMilliseconds;
        }

        private static string TitleFromSpecification(string specification)
        {
            string text = (specification ?? string.Empty).Trim();
            int newline = text.IndexOfAny(new char[] { '\r', '\n' });
            if (newline >= 0) text = text.Substring(0, newline).Trim();
            if (text.Length > 80) text = text.Substring(0, 77) + "...";
            return text.Length == 0 ? "Untitled feature" : text;
        }
    }

    internal static class RunCompletionAuthority
    {
        internal static bool IsConfirmed(int exitCode, PipelineReport report, string ownedRunId, string baselineRunId)
        {
            return exitCode == 0 && report != null && report.status == "completed"
                && !string.IsNullOrWhiteSpace(report.run_id)
                && !string.Equals(report.run_id, baselineRunId, StringComparison.Ordinal)
                && string.Equals(report.run_id, ownedRunId, StringComparison.Ordinal);
        }

        internal static bool CanAdvance(bool stopRequested, int exitCode, PipelineReport report, string ownedRunId, string baselineRunId)
        {
            return !stopRequested && IsConfirmed(exitCode, report, ownedRunId, baselineRunId);
        }

        internal static bool IsConfirmed(int exitCode, PipelineReport childReport, PipelineReport persistedReport, string baselineRunId)
        {
            return childReport != null && persistedReport != null
                && childReport.schema_version == 1 && persistedReport.schema_version == 1
                && IsConfirmed(exitCode, childReport, childReport.run_id, baselineRunId)
                && string.Equals(persistedReport.run_id, childReport.run_id, StringComparison.Ordinal)
                && string.Equals(persistedReport.status, childReport.status, StringComparison.Ordinal)
                && string.Equals(persistedReport.candidate_commit, childReport.candidate_commit, StringComparison.Ordinal)
                && string.Equals(persistedReport.candidate, childReport.candidate, StringComparison.Ordinal);
        }
    }

    internal sealed class ChatHistoryDocument
    {
        public int schema_version { get; set; }
        public List<ChatHistoryMessage> messages { get; set; }

        public ChatHistoryDocument()
        {
            schema_version = 1;
            messages = new List<ChatHistoryMessage>();
        }
    }

    internal sealed class ChatHistoryMessage
    {
        public string role { get; set; }
        public string text { get; set; }
        public string model { get; set; }
        public string endpoint { get; set; }
        public string provider { get; set; }
        public string reasoning_effort { get; set; }
        public long timestamp_ms { get; set; }
        public string request_id { get; set; }

        public ChatHistoryMessage() { }
    }

    internal static class ChatHistoryStore
    {
        internal const int MaximumMessages = 32;
        internal const int MaximumTextCharacters = 128 * 1024;

        internal static ChatHistoryDocument Load(string path)
        {
            if (!File.Exists(path)) return new ChatHistoryDocument();
            ChatHistoryDocument document = Serializer.New().Deserialize<ChatHistoryDocument>(File.ReadAllText(path, new UTF8Encoding(false, true)));
            Validate(document);
            return document;
        }

        internal static void Add(ChatHistoryDocument document, string role, string text, string model, string endpoint, string requestId)
        {
            Add(document, role, text, model, endpoint, requestId, RoleProvider.Local, "none");
        }

        internal static void Add(ChatHistoryDocument document, string role, string text, string model, string endpoint, string requestId, string provider, string reasoningEffort)
        {
            ChatHistoryMessage message = new ChatHistoryMessage();
            message.role = role;
            message.text = text ?? string.Empty;
            message.model = model ?? string.Empty;
            message.endpoint = endpoint ?? string.Empty;
            message.provider = string.IsNullOrWhiteSpace(provider) ? RoleProvider.Local : provider;
            message.reasoning_effort = reasoningEffort ?? string.Empty;
            message.timestamp_ms = AssemblyLineStore.NowMilliseconds();
            message.request_id = requestId ?? string.Empty;
            document.messages.Add(message);
            while (document.messages.Count > MaximumMessages) document.messages.RemoveAt(0);
            Validate(document);
        }

        internal static void Save(string path, ChatHistoryDocument document)
        {
            Validate(document);
            JobJson.WriteUtf8(path, Serializer.New().Serialize(document));
        }

        internal static void Validate(ChatHistoryDocument document)
        {
            if (document == null || document.schema_version != 1 || document.messages == null || document.messages.Count > MaximumMessages)
                throw new InvalidDataException("Chat history must use schema 1 and contain at most 32 messages.");
            foreach (ChatHistoryMessage message in document.messages)
            {
                if (message == null || (message.role != "owner" && message.role != "assistant" && message.role != "error"))
                    throw new InvalidDataException("Chat history contains an invalid role.");
                if (message.text == null || message.text.Length > MaximumTextCharacters)
                    throw new InvalidDataException("Chat history message exceeds its text limit.");
                if (message.role != "error")
                {
                    string provider = string.IsNullOrWhiteSpace(message.provider) ? RoleProvider.Local : message.provider;
                    if (!RoleProvider.IsValid(provider) || string.IsNullOrWhiteSpace(message.model)
                        || (provider == RoleProvider.Local && LoopbackEndpoint.Validate(message.endpoint) != null)
                        || (provider == RoleProvider.ChatGpt && (!string.IsNullOrEmpty(message.endpoint) || !ReasoningEffort.IsValid(message.reasoning_effort) || string.IsNullOrWhiteSpace(message.reasoning_effort))))
                        throw new InvalidDataException("Chat history contains invalid provider metadata.");
                }
            }
        }
    }

    internal static class Serializer
    {
        internal static JavaScriptSerializer New()
        {
            JavaScriptSerializer serializer = new JavaScriptSerializer();
            serializer.MaxJsonLength = 2 * 1024 * 1024;
            serializer.RecursionLimit = 64;
            return serializer;
        }
    }
}
