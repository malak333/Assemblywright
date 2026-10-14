using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Web.Script.Serialization;

namespace Assemblywright.Windows
{
    internal sealed class JobDocument
    {
        public string repository { get; set; }
        public string specification { get; set; }
        public List<string> allowed_files { get; set; }
        public ValidationDocument validation { get; set; }
        public string endpoint { get; set; }
        public string model { get; set; }
        public string orchestrator_provider { get; set; }
        public string orchestrator_model { get; set; }
        public string orchestrator_reasoning_effort { get; set; }
        public string reviewer_provider { get; set; }
        public string reviewer_model { get; set; }
        public string reasoning_effort { get; set; }
        public string reviewer_reasoning_effort { get; set; }
        public int max_attempts { get; set; }

        public JobDocument()
        {
            repository = string.Empty;
            specification = string.Empty;
            allowed_files = new List<string>();
            validation = new ValidationDocument();
            endpoint = "http://127.0.0.1:8080/v1";
            model = "qwen3.6-35b-a3b";
            orchestrator_provider = null;
            orchestrator_model = null;
            orchestrator_reasoning_effort = null;
            reviewer_provider = null;
            reviewer_model = null;
            reasoning_effort = "none";
            reviewer_reasoning_effort = "none";
            max_attempts = 3;
        }
    }

    internal sealed class ValidationDocument
    {
        public string program { get; set; }
        public List<string> args { get; set; }
        public int timeout_seconds { get; set; }

        public ValidationDocument()
        {
            program = string.Empty;
            args = new List<string>();
            timeout_seconds = 300;
        }
    }

    internal sealed class PipelineReport
    {
        public int schema_version { get; set; }
        public string run_id { get; set; }
        public string status { get; set; }
        public string stage { get; set; }
        public int attempt { get; set; }
        public string candidate { get; set; }
        public string candidate_commit { get; set; }
        public string message { get; set; }
        public object started_at_ms { get; set; }
        public object updated_at_ms { get; set; }
        public bool paused { get; set; }
    }

    internal static class JobJson
    {
        private static JavaScriptSerializer NewSerializer()
        {
            JavaScriptSerializer serializer = new JavaScriptSerializer();
            serializer.MaxJsonLength = 2 * 1024 * 1024;
            serializer.RecursionLimit = 32;
            return serializer;
        }

        internal static string Serialize(JobDocument document)
        {
            JavaScriptSerializer serializer = NewSerializer();
            Dictionary<string, object> value = serializer.DeserializeObject(serializer.Serialize(document)) as Dictionary<string, object>;
            if (value == null) throw new InvalidDataException("The job could not be serialized.");
            if (document == null || string.IsNullOrWhiteSpace(document.orchestrator_provider))
            {
                value.Remove("orchestrator_provider"); value.Remove("orchestrator_model"); value.Remove("orchestrator_reasoning_effort");
            }
            if (document == null || string.IsNullOrWhiteSpace(document.reviewer_provider)) value.Remove("reviewer_provider");
            return serializer.Serialize(value);
        }

        internal static JobDocument DeserializeJob(string json)
        {
            JobDocument document = NewSerializer().Deserialize<JobDocument>(json);
            if (document == null)
            {
                throw new InvalidDataException("The job JSON is empty.");
            }
            if (document.allowed_files == null)
            {
                document.allowed_files = new List<string>();
            }
            if (document.validation == null)
            {
                document.validation = new ValidationDocument();
            }
            if (document.validation.args == null)
            {
                document.validation.args = new List<string>();
            }
            return document;
        }

        internal static PipelineReport DeserializeReport(string json)
        {
            PipelineReport report = NewSerializer().Deserialize<PipelineReport>(json);
            if (report == null || string.IsNullOrWhiteSpace(report.status))
            {
                throw new InvalidDataException("The runner returned an invalid status document.");
            }
            return report;
        }

        internal static void WriteUtf8(string path, string json)
        {
            string parent = Path.GetDirectoryName(path);
            if (string.IsNullOrEmpty(parent))
            {
                throw new InvalidDataException("The job path has no parent directory.");
            }
            Directory.CreateDirectory(parent);
            string temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                File.WriteAllText(temporary, json, new UTF8Encoding(false, true));
                if (File.Exists(path))
                {
                    File.Replace(temporary, path, null);
                }
                else
                {
                    File.Move(temporary, path);
                }
            }
            finally
            {
                if (File.Exists(temporary))
                {
                    File.Delete(temporary);
                }
            }
        }
    }

    internal static class JobValidator
    {
        internal static string Validate(JobDocument job, bool requireExistingPaths)
        {
            if (job == null) return "The job is missing.";
            if (string.IsNullOrWhiteSpace(job.repository)) return "Choose a repository.";
            if (!Path.IsPathRooted(job.repository)) return "The repository path must be absolute.";
            if (requireExistingPaths && !Directory.Exists(job.repository)) return "The repository directory does not exist.";
            if (string.IsNullOrWhiteSpace(job.specification)) return "Enter a feature specification.";
            if (job.allowed_files == null || job.allowed_files.Count == 0) return "Enter at least one allowed file.";

            HashSet<string> seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (string item in job.allowed_files)
            {
                if (string.IsNullOrEmpty(item) || Path.IsPathRooted(item) || item.IndexOf(':') >= 0 || item.IndexOf('\\') >= 0)
                {
                    return "Allowed files must use normalized relative paths with forward slashes.";
                }
                string[] parts = item.Split('/');
                foreach (string part in parts)
                {
                    if (part.Length == 0 || part == "." || part == "..") return "Allowed files must use normalized relative paths.";
                }
                if (parts[0].Equals(".git", StringComparison.OrdinalIgnoreCase)) return ".git cannot be an allowed file.";
                if (!seen.Add(item)) return "Allowed files cannot contain duplicates.";
            }

            if (job.validation == null) return "Enter validation settings.";
            if (string.IsNullOrWhiteSpace(job.validation.program)) return "Choose a validation program.";
            if (!Path.IsPathRooted(job.validation.program)) return "The validation program path must be absolute.";
            if (requireExistingPaths && !File.Exists(job.validation.program)) return "The validation program does not exist.";
            if (job.validation.timeout_seconds < 1 || job.validation.timeout_seconds > 3600) return "Validation timeout must be between 1 and 3600 seconds.";
            if (job.max_attempts < 1 || job.max_attempts > 20) return "Attempts must be between 1 and 20.";
            if (string.IsNullOrWhiteSpace(job.model)) return "Enter the local model name.";
            if (!ReasoningEffort.IsValid(job.reasoning_effort)) return "Choose a supported local implementation reasoning level.";
            if (!string.IsNullOrWhiteSpace(job.orchestrator_provider))
            {
                if (!RoleProvider.IsValid(job.orchestrator_provider)) return "Choose local AI or ChatGPT for the orchestrator.";
                if (string.IsNullOrWhiteSpace(job.orchestrator_model) || job.orchestrator_model.Length > 200) return "Choose an orchestrator model of at most 200 characters.";
                if (!ReasoningEffort.IsValid(job.orchestrator_reasoning_effort) || string.IsNullOrWhiteSpace(job.orchestrator_reasoning_effort)) return "Choose a supported orchestrator reasoning level.";
            }
            if (!string.IsNullOrWhiteSpace(job.reviewer_provider) && !RoleProvider.IsValid(job.reviewer_provider)) return "Choose local AI or ChatGPT for the reviewer.";
            if (job.reviewer_provider == "chatgpt" && string.IsNullOrWhiteSpace(job.reviewer_model)) return "Choose a ChatGPT reviewer model.";
            if (!string.IsNullOrWhiteSpace(job.reviewer_model) && job.reviewer_model.Length > 200) return "Reviewer model names must be at most 200 characters.";
            if (!ReasoningEffort.IsValid(job.reviewer_reasoning_effort)) return "Choose a supported reviewer reasoning level.";

            return LoopbackEndpoint.Validate(job.endpoint);
        }

        internal static string ValidateForForm(JobDocument job, bool requireExistingPaths)
        {
            string error = Validate(job, requireExistingPaths);
            if (error != null) return error;
            if (job.validation.args != null)
            {
                foreach (string argument in job.validation.args)
                {
                    if (string.IsNullOrEmpty(argument))
                        return "Empty validation arguments cannot be represented by the one-argument-per-line editor. Remove the empty argument or use a wrapper program.";
                    if (argument.IndexOf('\r') >= 0 || argument.IndexOf('\n') >= 0)
                        return "Validation arguments containing line breaks cannot be represented by the one-argument-per-line editor. Remove the line break or use a wrapper program.";
                }
            }
            return null;
        }
    }

    internal static class RoleProvider
    {
        internal const string Local = "local";
        internal const string ChatGpt = "chatgpt";

        internal static bool IsValid(string value)
        {
            return value == Local || value == ChatGpt;
        }

        internal static string DisplayName(string value)
        {
            return value == ChatGpt ? "ChatGPT" : "Local AI";
        }
    }

    internal static class ReasoningEffort
    {
        internal static readonly string[] Values = new string[] { "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra" };

        internal static bool IsValid(string value)
        {
            if (string.IsNullOrWhiteSpace(value)) return true;
            foreach (string allowed in Values) if (string.Equals(value, allowed, StringComparison.Ordinal)) return true;
            return false;
        }

        internal static string Normalize(string value)
        {
            return IsValid(value) && !string.IsNullOrWhiteSpace(value) ? value : "none";
        }
    }

    internal enum ReportBindingKind
    {
        None,
        OwnRunning,
        OwnCompleted,
        OwnTerminal,
        ExternalRunning,
        PreviousCompleted,
        ExternalTerminal
    }

    internal static class ReportBindings
    {
        internal static ReportBindingKind Classify(PipelineReport report, string ownedRunId, bool processStartedByThisGui)
        {
            if (report == null || report.status == "idle") return ReportBindingKind.None;
            bool owned = !string.IsNullOrEmpty(ownedRunId) && string.Equals(report.run_id, ownedRunId, StringComparison.Ordinal);
            if (owned && report.status == "running") return ReportBindingKind.OwnRunning;
            if (owned && report.status == "completed") return ReportBindingKind.OwnCompleted;
            if (owned) return ReportBindingKind.OwnTerminal;
            if (report.status == "running") return processStartedByThisGui ? ReportBindingKind.OwnRunning : ReportBindingKind.ExternalRunning;
            if (report.status == "completed") return ReportBindingKind.PreviousCompleted;
            return ReportBindingKind.ExternalTerminal;
        }

        internal static bool TryExtractTerminalReport(string standardOutput, out PipelineReport report)
        {
            report = null;
            if (string.IsNullOrWhiteSpace(standardOutput)) return false;
            int index = standardOutput.LastIndexOf('{');
            while (index >= 0)
            {
                string candidate = standardOutput.Substring(index).Trim();
                try
                {
                    PipelineReport parsed = JobJson.DeserializeReport(candidate);
                    Guid runId;
                    bool terminal = parsed.status == "completed" || parsed.status == "failed"
                        || parsed.status == "cancelled" || parsed.status == "interrupted";
                    if (parsed.schema_version == 1 && terminal
                        && Guid.TryParseExact(parsed.run_id, "D", out runId)
                        && string.Equals(parsed.run_id, runId.ToString("D"), StringComparison.Ordinal))
                    {
                        report = parsed;
                        return true;
                    }
                }
                catch (Exception) { }
                if (index == 0) break;
                index = standardOutput.LastIndexOf('{', index - 1);
            }
            return false;
        }
    }
}
