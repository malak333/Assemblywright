using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace Assemblywright.Windows
{
    internal sealed class LaunchOptions
    {
        internal bool TestMode;
        internal bool SelfTest;
        internal string SelfTestOutput;
        internal string StateDirectory;
        internal string JobPath;
        internal string ScreenshotPath;

        internal static LaunchOptions Parse(string[] arguments)
        {
            LaunchOptions options = new LaunchOptions();
            options.StateDirectory = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Assemblywright", "local-pipeline");
            for (int index = 0; index < arguments.Length; index++)
            {
                string argument = arguments[index];
                if (argument == "--test-mode") options.TestMode = true;
                else if (argument == "--self-test")
                {
                    options.SelfTest = true;
                    if (index + 1 < arguments.Length && !arguments[index + 1].StartsWith("--", StringComparison.Ordinal))
                        options.SelfTestOutput = arguments[++index];
                }
                else if (argument == "--state-dir") options.StateDirectory = RequireValue(arguments, ref index, argument);
                else if (argument == "--job") options.JobPath = RequireValue(arguments, ref index, argument);
                else if (argument == "--screenshot") options.ScreenshotPath = RequireValue(arguments, ref index, argument);
                else throw new ArgumentException("Unknown argument: " + argument);
            }
            options.StateDirectory = Path.GetFullPath(options.StateDirectory);
            if (!string.IsNullOrEmpty(options.JobPath)) options.JobPath = Path.GetFullPath(options.JobPath);
            if (!string.IsNullOrEmpty(options.ScreenshotPath)) options.ScreenshotPath = Path.GetFullPath(options.ScreenshotPath);
            if (!string.IsNullOrEmpty(options.SelfTestOutput)) options.SelfTestOutput = Path.GetFullPath(options.SelfTestOutput);
            return options;
        }

        private static string RequireValue(string[] arguments, ref int index, string name)
        {
            if (index + 1 >= arguments.Length) throw new ArgumentException(name + " requires a value.");
            return arguments[++index];
        }
    }

    internal static class Program
    {
        [STAThread]
        private static int Main(string[] arguments)
        {
            LaunchOptions options;
            try { options = LaunchOptions.Parse(arguments); }
            catch (Exception exception)
            {
                Console.Error.WriteLine(exception.Message);
                return 2;
            }

            if (options.SelfTest) return SelfTests.Run(options.SelfTestOutput);

            bool created;
            using (Mutex mutex = new Mutex(true, MutexName(options.StateDirectory), out created))
            {
                if (!created)
                {
                    if (!options.TestMode)
                        MessageBox.Show("Assemblywright is already open for this state directory.", "Assemblywright", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return 3;
                }

                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                MainForm form = null;
                Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
                Application.ThreadException += delegate(object sender, ThreadExceptionEventArgs eventArgs)
                {
                    if (form != null) form.SurfaceUnhandled(eventArgs.Exception);
                };
                try
                {
                    form = new MainForm(options.StateDirectory, options.JobPath, options.TestMode, options.ScreenshotPath);
                    Application.Run(form);
                    return 0;
                }
                catch (Exception exception)
                {
                    Console.Error.WriteLine(exception);
                    if (!options.TestMode)
                        MessageBox.Show(exception.Message, "Assemblywright could not start", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    return 1;
                }
            }
        }

        private static string MutexName(string stateDirectory)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(Path.GetFullPath(stateDirectory).ToUpperInvariant());
            using (SHA256 digest = SHA256.Create())
            {
                byte[] hash = digest.ComputeHash(bytes);
                StringBuilder text = new StringBuilder("Local\\Assemblywright.Windows.Gui.");
                for (int index = 0; index < 12; index++) text.Append(hash[index].ToString("x2"));
                return text.ToString();
            }
        }
    }

    internal static class SelfTests
    {
        internal static int Run(string outputPath)
        {
            List<string> passed = new List<string>();
            try
            {
                JobDocument job = ValidJob();
                string serialized = JobJson.Serialize(job);
                JobDocument roundTrip = JobJson.DeserializeJob(serialized);
                Require(roundTrip.repository == job.repository && roundTrip.allowed_files.Count == 2
                    && roundTrip.validation.args[1] == "argument with spaces", "job round trip");
                string sharedReadPath = Path.Combine(Path.GetTempPath(), "assemblywright-gui-shared-read-" + Guid.NewGuid().ToString("N") + ".json");
                try
                {
                    File.WriteAllText(sharedReadPath, "{\"status\":\"running\"}", new UTF8Encoding(false));
                    Require(SharedFile.ReadUtf8Json(sharedReadPath) == "{\"status\":\"running\"}", "shared JSON read changed bytes");
                    using (FileStream shared = SharedFile.OpenRead(sharedReadPath))
                    {
                        File.Delete(sharedReadPath);
                        Require(!File.Exists(sharedReadPath), "shared JSON read blocked atomic replacement semantics");
                    }
                    File.WriteAllBytes(sharedReadPath, new byte[] { 0xef, 0xbb, 0xbf, (byte)'{', (byte)'}' });
                    Require(SharedFile.ReadUtf8Json(sharedReadPath) == "{}", "shared JSON read did not preserve UTF-8 BOM compatibility");
                    File.WriteAllBytes(sharedReadPath, new byte[] { 0xff });
                    bool invalidUtf8Rejected = false;
                    try { SharedFile.ReadUtf8Json(sharedReadPath); }
                    catch (DecoderFallbackException) { invalidUtf8Rejected = true; }
                    Require(invalidUtf8Rejected, "shared JSON read accepted invalid UTF-8");
                    using (FileStream oversized = new FileStream(sharedReadPath, FileMode.Create, FileAccess.Write, FileShare.None)) oversized.SetLength(SharedFile.MaximumJsonBytes + 1L);
                    bool oversizedRejected = false;
                    try { SharedFile.ReadUtf8Json(sharedReadPath); }
                    catch (InvalidDataException) { oversizedRejected = true; }
                    Require(oversizedRejected, "shared JSON read accepted an oversized document");
                }
                finally { if (File.Exists(sharedReadPath)) File.Delete(sharedReadPath); }
                passed.Add("job-round-trip");

                Require(WindowsCommandLine.Quote("") == "\"\"", "empty argument quote");
                Require(WindowsCommandLine.Quote("plain") == "plain", "plain argument quote");
                Require(WindowsCommandLine.Quote("a b") == "\"a b\"", "space argument quote");
                Require(WindowsCommandLine.Quote("a\"b") == "\"a\\\"b\"", "embedded quote");
                Require(WindowsCommandLine.Quote("C:\\space path\\") == "\"C:\\space path\\\\\"", "trailing slash quote");
                passed.Add("windows-argument-quoting");

                Require(JobValidator.Validate(job, false) == null, "valid job rejected");
                job.endpoint = "https://example.com/v1";
                Require(JobValidator.Validate(job, false) != null, "cloud endpoint accepted");
                job = ValidJob();
                job.allowed_files[0] = "../escape.cs";
                Require(JobValidator.Validate(job, false) != null, "traversal accepted");
                job = ValidJob();
                job.max_attempts = 0;
                Require(JobValidator.Validate(job, false) != null, "invalid attempt count accepted");
                job = ValidJob();
                job.validation.args.Insert(0, string.Empty);
                Require(JobValidator.ValidateForForm(job, false) != null, "empty validation argument was silently accepted by the form");
                job = ValidJob();
                job.validation.args.Insert(0, "first\r\nsecond");
                Require(JobValidator.ValidateForForm(job, false) != null, "multiline validation argument was silently accepted by the form");
                passed.Add("job-validation-negative-paths");

                PipelineReport report = new PipelineReport();
                report.run_id = "run-a";
                report.status = "running";
                Require(ReportBindings.Classify(report, null, false) == ReportBindingKind.ExternalRunning, "external run claimed");
                Require(ReportBindings.Classify(report, "run-a", true) == ReportBindingKind.OwnRunning, "owned run lost");
                report.status = "completed";
                Require(ReportBindings.Classify(report, "run-a", false) == ReportBindingKind.OwnCompleted, "owned completion lost");
                Require(ReportBindings.Classify(report, null, false) == ReportBindingKind.PreviousCompleted, "previous completion mislabeled");
                passed.Add("current-report-bindings");

                BoundedLog log = new BoundedLog(128);
                log.Append(new string('x', 200));
                log.Append("\u20ac\u20ac\u20ac");
                Require(log.ByteCount <= 128, "activity log exceeded byte boundary");
                passed.Add("bounded-log");

                GuiSettings settings = new GuiSettings();
                Require(GuiSettingsStore.Validate(settings) != null, "unconfigured ChatGPT settings accepted without an advertised model pair");
                settings.orchestrator_model = "chat-model"; settings.orchestrator_reasoning_effort = "high"; settings.reviewer_model = "chat-model"; settings.reviewer_reasoning_effort = "high";
                Require(GuiSettingsStore.Validate(settings) == null, "configured default settings rejected");
                settings.endpoint = "https://127.0.0.1/v1";
                Require(GuiSettingsStore.Validate(settings) != null, "TLS/cloud-style endpoint accepted");
                settings.endpoint = "http://owner@127.0.0.1/v1";
                Require(GuiSettingsStore.Validate(settings) != null, "endpoint user information accepted");
                settings = new GuiSettings();
                settings.default_attempts = 21;
                Require(GuiSettingsStore.Validate(settings) != null, "oversized default attempts accepted");
                passed.Add("gui-settings-negative-paths");

                GuiSettings roleSettings = new GuiSettings();
                roleSettings.model = "implementation-model"; roleSettings.reasoning_effort = "high";
                roleSettings.orchestrator_provider = RoleProvider.ChatGpt; roleSettings.orchestrator_model = "orchestrator-model"; roleSettings.orchestrator_reasoning_effort = "xhigh";
                roleSettings.reviewer_provider = RoleProvider.ChatGpt; roleSettings.reviewer_model = "reviewer-model"; roleSettings.reviewer_reasoning_effort = "high"; roleSettings.revision = 7;
                Require(GuiSettingsStore.Validate(roleSettings) == null, "role settings rejected");
                JobDocument roleJob = ValidJob(); roleJob.model = roleSettings.model; roleJob.reasoning_effort = roleSettings.reasoning_effort; roleJob.orchestrator_provider = roleSettings.orchestrator_provider; roleJob.orchestrator_model = roleSettings.orchestrator_model; roleJob.orchestrator_reasoning_effort = roleSettings.orchestrator_reasoning_effort; roleJob.reviewer_provider = roleSettings.reviewer_provider; roleJob.reviewer_model = roleSettings.reviewer_model; roleJob.reviewer_reasoning_effort = roleSettings.reviewer_reasoning_effort;
                JobDocument roleRoundTrip = JobJson.DeserializeJob(JobJson.Serialize(roleJob));
                Require(roleRoundTrip.model == "implementation-model" && roleRoundTrip.orchestrator_provider == RoleProvider.ChatGpt && roleRoundTrip.orchestrator_model == "orchestrator-model" && roleRoundTrip.orchestrator_reasoning_effort == "xhigh"
                    && roleRoundTrip.reviewer_provider == RoleProvider.ChatGpt && roleRoundTrip.reviewer_model == "reviewer-model" && roleRoundTrip.reviewer_reasoning_effort == "high", "role snapshot changed during job round trip");
                JobDocument legacyRoleJob = ValidJob(); legacyRoleJob.model = "legacy-model"; legacyRoleJob.reviewer_model = null;
                JobDocument legacyRoundTrip = JobJson.DeserializeJob(JobJson.Serialize(legacyRoleJob));
                Require(legacyRoundTrip.orchestrator_provider == null && legacyRoundTrip.reviewer_provider == null && RoleSettings.ReviewerModelForJob(legacyRoleJob, roleSettings) == "legacy-model", "legacy job provider semantics or reviewer fallback changed");
                roleJob.reasoning_effort = "unsupported"; Require(JobValidator.Validate(roleJob, false) != null, "unsupported reasoning accepted");
                roleJob = ValidJob(); roleJob.orchestrator_provider = "unknown"; roleJob.orchestrator_model = "model"; roleJob.orchestrator_reasoning_effort = "high"; Require(JobValidator.Validate(roleJob, false) != null, "unknown orchestrator provider accepted");
                string legacySettingsPath = Path.Combine(Path.GetTempPath(), "assemblywright-gui-settings-" + Guid.NewGuid().ToString("N") + ".json");
                try
                {
                    File.WriteAllText(legacySettingsPath, "{\"schema_version\":1,\"endpoint\":\"http://127.0.0.1:8080/v1\",\"model\":\"legacy-local\",\"reviewer_model\":\"legacy-review\",\"reasoning_effort\":\"low\",\"reviewer_reasoning_effort\":\"high\",\"revision\":4,\"default_attempts\":3,\"timeout_seconds\":300}", new UTF8Encoding(false));
                    GuiSettings migrated = GuiSettingsStore.Load(legacySettingsPath);
                    Require(migrated.orchestrator_provider == RoleProvider.ChatGpt && string.IsNullOrEmpty(migrated.orchestrator_model) && migrated.reviewer_provider == RoleProvider.ChatGpt && string.IsNullOrEmpty(migrated.reviewer_model), "schema-1 settings did not migrate future roles to catalog-bound ChatGPT choices");
                }
                finally { if (File.Exists(legacySettingsPath)) File.Delete(legacySettingsPath); }
                ChatGptModelOption chatModel = new ChatGptModelOption("chat-model", "Chat model", new string[] { "low", "high" }, "high");
                Require(chatModel.DefaultReasoningEffort == "high" && chatModel.ReasoningEfforts.SequenceEqual(new string[] { "low", "high" }), "ChatGPT catalog model defaults changed");
                bool invalidDefaultRejected = false; try { new ChatGptModelOption("bad", "Bad", new string[] { "low" }, "high"); } catch (InvalidDataException) { invalidDefaultRejected = true; }
                Require(invalidDefaultRejected, "unadvertised ChatGPT default effort accepted");
                SettingsDialog lockedSettings = new SettingsDialog(roleSettings, true, new List<LocalModelOption> { new LocalModelOption("implementation-model", new string[] { "high" }) }, new List<ChatGptModelOption> { chatModel });
                try
                {
                    Require(!lockedSettings.Controls.Find("SettingsSaveButton", true).Single().Enabled, "locked settings allowed a role change");
                    Require(lockedSettings.Controls.Find("SettingsOrchestratorProviderBox", true).Length == 0 && lockedSettings.Controls.Find("SettingsReviewerProviderBox", true).Length == 0, "Mac role settings exposed a provider selector");
                }
                finally { lockedSettings.Dispose(); }
                string savedOrchestrator = roleSettings.orchestrator_model; roleSettings.orchestrator_model = "missing-chat-model"; roleSettings.reviewer_model = "chat-model"; roleSettings.reviewer_reasoning_effort = "high";
                SettingsDialog unavailableSettings = new SettingsDialog(roleSettings, false, new List<LocalModelOption> { new LocalModelOption("implementation-model", new string[] { "high" }) }, new List<ChatGptModelOption> { chatModel });
                try
                {
                    Require(Convert.ToString(((ComboBox)unavailableSettings.Controls.Find("SettingsOrchestratorModelBox", true).Single()).SelectedItem) == "missing-chat-model (unavailable)", "saved unavailable ChatGPT model was silently substituted");
                    Require(Convert.ToString(((ComboBox)unavailableSettings.Controls.Find("SettingsReviewerModelBox", true).Single()).SelectedItem) == "Chat model", "ChatGPT model display name was not shown");
                    Require(Convert.ToString(((ComboBox)unavailableSettings.Controls.Find("SettingsReviewerReasoningBox", true).Single()).SelectedItem) == "High", "Mac reasoning label was not shown");
                    Require(!unavailableSettings.Controls.Find("SettingsSaveButton", true).Single().Enabled, "unadvertised ChatGPT pair could be saved");
                }
                finally { unavailableSettings.Dispose(); roleSettings.orchestrator_model = savedOrchestrator; }
                ChatReply matchedReply = ChatGptChatClient.ParseResponse("{\"status\":\"completed\",\"text\":\"Plan\",\"provider\":\"chatgpt\",\"model\":\"chat-model\",\"reasoning_effort\":\"high\"}", "chat-model", "high");
                Require(matchedReply.Text == "Plan", "matching ChatGPT response rejected");
                bool mismatchedReplyRejected = false; try { ChatGptChatClient.ParseResponse("{\"status\":\"completed\",\"text\":\"Plan\",\"provider\":\"chatgpt\",\"model\":\"other\",\"reasoning_effort\":\"high\"}", "chat-model", "high"); } catch (InvalidDataException) { mismatchedReplyRejected = true; }
                Require(mismatchedReplyRejected, "mismatched ChatGPT response binding accepted");
                ChatHistoryMessage localHistory = new ChatHistoryMessage(); localHistory.role = "owner"; localHistory.text = "LOCAL_SECRET_MARKER"; localHistory.provider = RoleProvider.Local; localHistory.model = "local-model";
                ChatHistoryMessage cloudHistory = new ChatHistoryMessage(); cloudHistory.role = "assistant"; cloudHistory.text = "CLOUD_CONTEXT_MARKER"; cloudHistory.provider = RoleProvider.ChatGpt; cloudHistory.model = "chat-model";
                ChatHistoryMessage otherCloudModel = new ChatHistoryMessage(); otherCloudModel.role = "owner"; otherCloudModel.text = "OTHER_MODEL_MARKER"; otherCloudModel.provider = RoleProvider.ChatGpt; otherCloudModel.model = "other-model";
                string planningPrompt = ChatGptChatClient.BuildPlanningPrompt(new List<ChatHistoryMessage> { localHistory, cloudHistory, otherCloudModel }, "Current", "New request", "chat-model");
                Require(!planningPrompt.Contains("LOCAL_SECRET_MARKER") && !planningPrompt.Contains("OTHER_MODEL_MARKER") && planningPrompt.Contains("CLOUD_CONTEXT_MARKER"), "ChatGPT prompt crossed a prior provider or model boundary");
                passed.Add("provider-settings-schema-freeze-and-chat-bindings");

                PermissionsDocument permission = new PermissionsDocument(); permission.schema_version = 1; permission.mode = "full"; permission.revision = 0; permission.execution_host = "windows"; permission.available = true;
                Require(NativeGuiControl.IsValidPermissions(permission), "default full permission rejected");
                permission.mode = "ask"; permission.revision = 4;
                Require(NativeGuiControl.AcknowledgesPermissionChange(permission, "ask", 3), "permission revision acknowledgement rejected");
                Require(!NativeGuiControl.AcknowledgesPermissionChange(permission, "ask", 2), "stale permission revision accepted");
                ApprovalDocument expectedApproval = new ApprovalDocument(); expectedApproval.schema_version = 1; expectedApproval.id = Guid.NewGuid().ToString("D"); expectedApproval.token = new string('a', 64); expectedApproval.revision = 2; expectedApproval.run_id = Guid.NewGuid().ToString("D"); expectedApproval.action = "run_validation"; expectedApproval.digest = new string('b', 64); expectedApproval.detail = "Run validation"; expectedApproval.requested_at_ms = 1234L;
                expectedApproval.status = "pending"; Require(NativeGuiControl.IsValidPendingApproval(expectedApproval), "exact pending approval rejected"); expectedApproval.status = null; Require(!NativeGuiControl.IsValidPendingApproval(expectedApproval), "status-less approval accepted"); expectedApproval.status = "pending";
                ApprovalDocument approvalAcknowledgement = new ApprovalDocument(); approvalAcknowledgement.id = expectedApproval.id; approvalAcknowledgement.token = expectedApproval.token; approvalAcknowledgement.revision = expectedApproval.revision; approvalAcknowledgement.run_id = expectedApproval.run_id; approvalAcknowledgement.action = expectedApproval.action; approvalAcknowledgement.digest = expectedApproval.digest; approvalAcknowledgement.detail = expectedApproval.detail; approvalAcknowledgement.requested_at_ms = expectedApproval.requested_at_ms; approvalAcknowledgement.status = "approved"; approvalAcknowledgement.approved = true; approvalAcknowledgement.decided_at_ms = 1235L;
                Require(NativeGuiControl.AcknowledgesApproval(approvalAcknowledgement, expectedApproval, true), "exact approval acknowledgement rejected");
                approvalAcknowledgement.digest = new string('c', 64); Require(!NativeGuiControl.AcknowledgesApproval(approvalAcknowledgement, expectedApproval, true), "wrong-digest approval acknowledgement accepted");
                LocalModelOption restrictedModel = new LocalModelOption("model-a", new string[] { "low", "high", "bogus" }); Require(restrictedModel.AvailableEfforts.SequenceEqual(new string[] { "low", "high" }), "model-specific reasoning efforts changed");
                HeaderIconButton githubIcon = new HeaderIconButton(HeaderIconKind.GitHub, "GitHubButton", "GitHub publication");
                HeaderIconButton permissionIcon = new HeaderIconButton(HeaderIconKind.Shield, "SafetyButton", "Permissions");
                HeaderIconButton settingsIcon = new HeaderIconButton(HeaderIconKind.Gear, "SettingsButton", "Settings");
                Require(githubIcon.AccessibleName == "GitHub publication" && permissionIcon.AccessibleName == "Permissions" && settingsIcon.AccessibleName == "Settings", "Mac header accessibility contract changed");
                passed.Add("mac-header-and-permissions-contract");

                PipelineReport publicationReport = new PipelineReport(); publicationReport.run_id = Guid.NewGuid().ToString("D"); publicationReport.candidate_commit = new string('a', 40);
                GitHubConnection publicationConnection = new GitHubConnection(); publicationConnection.revision = 9; publicationConnection.project = "C:\\source"; publicationConnection.repository = "owner/repository"; publicationConnection.base_branch = "main"; publicationConnection.source_head = new string('d', 40);
                GitHubPublication publication = new GitHubPublication(); publication.status = "succeeded"; publication.run_id = publicationReport.run_id; publication.connection_revision = publicationConnection.revision; publication.completion_revision = 10; publication.candidate_commit = publicationReport.candidate_commit; publication.project = publicationConnection.project; publication.repository = publicationConnection.repository; publication.base_branch = publicationConnection.base_branch; publication.source_head = publicationConnection.source_head; publication.branch = "assemblywright/" + publicationReport.run_id; publication.pull_request_number = 1; publication.pull_request_url = "https://github.com/owner/repository/pull/1"; publication.merge_commit = new string('b', 40);
                Require(GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "exact publication binding rejected");
                publication.project = @"\\?\C:\source";
                Require(GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "verbatim-drive publication path rejected against an exact normal project");
                publicationConnection.project = @"\\?\C:\source";
                Require(GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "matching verbatim-drive publication paths rejected");
                publication.project = @"\\?\UNC\server\share\source"; publicationConnection.project = @"\\server\share\source";
                Require(GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "verbatim UNC publication path rejected against an exact normal UNC project");
                publication.project = @"C:\source"; publicationConnection.project = @"C:\source";
                string normalizedProject;
                foreach (string invalidProject in new string[] { "relative", @"C:relative", @"C:\source.", @"C:\source:stream", @"C:\CON", @"\\?\C:relative", @"\\?\C:\source.", @"\\?\C:\source ", @"\\?\C:\source:stream", @"\\?\C:\source\..\other", @"\\?\C:\CON", @"\\?\C:/source", @"\\?\UNC\server", @"\\?\UNC\server\share\source.", @"\\?\GLOBALROOT\Device\HarddiskVolume1\source", @"\\.\C:\source", @"\??\C:\source", " C:\\source", "C:\\bad<path" })
                {
                    Require(!GitHubNative.TryNormalizeProjectPath(invalidProject, out normalizedProject), "ambiguous or malformed publication project path accepted: " + invalidProject);
                    publication.project = invalidProject;
                    Require(!GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "invalid publication project path retained publication authority: " + invalidProject);
                }
                publication.project = @"C:\source";
                using (GitHubDialog connectionDialog = new GitHubDialog(@"C:\fallback", false))
                {
                    GitHubResponse verbatimConnection = new GitHubResponse(); verbatimConnection.revision = 17; verbatimConnection.connection = new GitHubConnection(); verbatimConnection.connection.project = @"\\?\C:\source"; verbatimConnection.connection.repository = "owner/repository"; verbatimConnection.connection.base_branch = "main";
                    connectionDialog.ApplyConnection(verbatimConnection);
                    Require(connectionDialog.ProjectPath.Text == Path.GetFullPath(@"C:\source") && connectionDialog.ConnectionRevision == 17, "GitHub panel retained a legacy-incompatible verbatim drive path");
                    verbatimConnection.revision = 18; verbatimConnection.connection.project = @"\\?\C:\source:stream";
                    connectionDialog.ApplyConnection(verbatimConnection);
                    Require(connectionDialog.ProjectPath.Text == Path.GetFullPath(@"C:\source") && connectionDialog.ConnectionRevision == 17 && !connectionDialog.Disconnect.Enabled && connectionDialog.Status.Text.IndexOf("invalid", StringComparison.OrdinalIgnoreCase) >= 0, "GitHub panel accepted or retained an alternate-stream native project path");
                }
                publication.run_id = Guid.NewGuid().ToString("D"); Require(!GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "wrong-run publication accepted");
                publication.run_id = publicationReport.run_id; publication.candidate_commit = new string('c', 40); Require(!GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "wrong-candidate publication accepted");
                publication.candidate_commit = publicationReport.candidate_commit; publication.connection_revision++; Require(!GitHubNative.PublicationMatches(publication, publicationReport, publicationConnection), "wrong-connection-revision publication accepted"); publication.connection_revision = publicationConnection.revision;
                GitHubResponse durablePublication = new GitHubResponse(); durablePublication.status = "ok"; durablePublication.revision = publication.completion_revision; durablePublication.publication_receipts[publication.run_id] = new GitHubPublicationReceipt { verified = true, revision = publication.completion_revision };
                Require(GitHubNative.DurableReceiptMatches(durablePublication, publication.run_id, publication), "exact durable publication receipt rejected");
                durablePublication.publication_receipts[publication.run_id].verified = false; Require(!GitHubNative.DurableReceiptMatches(durablePublication, publication.run_id, publication), "unverified publication receipt accepted");
                durablePublication.publication_receipts[publication.run_id].verified = true; durablePublication.revision++; Require(GitHubNative.DurableReceiptMatches(durablePublication, publication.run_id, publication), "verified receipt rejected after a later global revision");
                durablePublication.publication_receipts[publication.run_id].revision++; Require(!GitHubNative.DurableReceiptMatches(durablePublication, publication.run_id, publication), "wrong receipt completion revision accepted"); durablePublication.publication_receipts[publication.run_id].revision = publication.completion_revision;
                GitHubConnection advancedConnection = new GitHubConnection(); advancedConnection.revision = publication.completion_revision; advancedConnection.project = publicationConnection.project; advancedConnection.repository = publicationConnection.repository; advancedConnection.base_branch = publicationConnection.base_branch; advancedConnection.source_head = publication.merge_commit;
                Require(!GitHubNative.PublicationMatches(publication, publicationReport, advancedConnection), "normal publication accepted an advanced connection");
                Require(GitHubNative.ReconciledPublicationMatches(publication, publicationReport, advancedConnection), "reconciliation rejected a receipt-confirmed fast-forwarded connection");
                advancedConnection.source_head = new string('e', 40); Require(!GitHubNative.ReconciledPublicationMatches(publication, publicationReport, advancedConnection), "reconciliation accepted a different advanced source head");
                passed.Add("github-publication-binding");

                PipelineReport completion = new PipelineReport();
                completion.status = "completed";
                completion.run_id = "new-run";
                Require(RunCompletionAuthority.IsConfirmed(0, completion, "new-run", "old-run"), "owned completion rejected");
                Require(!RunCompletionAuthority.IsConfirmed(0, completion, "other-run", "old-run"), "unowned completion accepted");
                Require(!RunCompletionAuthority.IsConfirmed(0, completion, "new-run", "new-run"), "old report accepted as new completion");
                Require(!RunCompletionAuthority.CanAdvance(true, 0, completion, "new-run", "old-run"), "stop request allowed automatic queue advance");
                Require(!RunCompletionAuthority.CanAdvance(false, 1, completion, "new-run", "old-run"), "failed process allowed automatic queue advance");
                string childId = Guid.NewGuid().ToString("D");
                PipelineReport childCompletion = new PipelineReport();
                childCompletion.schema_version = 1;
                childCompletion.status = "completed";
                childCompletion.run_id = childId;
                childCompletion.candidate = "candidate-a";
                childCompletion.candidate_commit = new string('a', 40);
                PipelineReport persistedCompletion = new PipelineReport();
                persistedCompletion.schema_version = 1;
                persistedCompletion.status = "completed";
                persistedCompletion.run_id = childId;
                persistedCompletion.candidate = childCompletion.candidate;
                persistedCompletion.candidate_commit = childCompletion.candidate_commit;
                Require(RunCompletionAuthority.IsConfirmed(0, childCompletion, persistedCompletion, Guid.NewGuid().ToString("D")), "child/persisted completion cross-check rejected");
                persistedCompletion.run_id = Guid.NewGuid().ToString("D");
                Require(!RunCompletionAuthority.IsConfirmed(0, childCompletion, persistedCompletion, "old"), "external latest report stole child ownership");
                string terminalOutput = "Demonstration heading\r\n{\"schema_version\":1,\"run_id\":\"" + childId
                    + "\",\"status\":\"completed\",\"stage\":\"completed\",\"attempt\":1,\"candidate\":\"candidate-a\",\"candidate_commit\":\""
                    + new string('a', 40) + "\",\"message\":\"done\",\"history\":[{\"stage\":\"validation\",\"status\":\"completed\"}]}\r\n";
                PipelineReport extracted;
                Require(ReportBindings.TryExtractTerminalReport(terminalOutput, out extracted) && extracted.run_id == childId, "terminal child report was not extracted after launcher output");
                Require(!ReportBindings.TryExtractTerminalReport("{\"stage\":\"validation\",\"status\":\"completed\"}", out extracted), "nested stage was accepted as a child report");
                passed.Add("assembly-line-completion-authority");

                StartOperationGate startGate = new StartOperationGate();
                Require(startGate.TryEnter(), "first start gate entry failed");
                Require(!startGate.TryEnter(), "reentrant start gate entry succeeded");
                startGate.Exit();
                Require(startGate.TryEnter(), "released start gate could not be reacquired");
                startGate.Exit();
                Require(PrelaunchAuthority.MayLaunch(false, false), "clear prelaunch authority rejected");
                Require(!PrelaunchAuthority.MayLaunch(true, false), "Stop Line did not dominate prelaunch");
                Require(!PrelaunchAuthority.MayLaunch(false, true), "window close did not dominate prelaunch");
                passed.Add("start-operation-reentrancy");

                System.Diagnostics.Process cleanupChild = ProcessRunner.StartStreaming(
                    "cmd.exe",
                    new string[] { "/d", "/c", "ping -n 30 127.0.0.1 > nul" },
                    null,
                    null);
                Require(ProcessRunner.TryTerminateAndDispose(cleanupChild, 5000), "partially launched child cleanup was not confirmed");
                passed.Add("process-lifecycle-cleanup");

                string temporary = Path.Combine(Path.GetTempPath(), "assemblywright-gui-self-test-" + Guid.NewGuid().ToString("N"));
                Directory.CreateDirectory(temporary);
                try
                {
                    string validationProgram = Path.Combine(temporary, "validate.cmd");
                    File.WriteAllText(validationProgram, "@exit /b 0", new UTF8Encoding(false));
                    JobDocument first = ValidJob();
                    first.repository = temporary;
                    first.validation.program = validationProgram;
                    first.specification = "First approved feature.";
                    JobDocument second = ValidJob();
                    second.repository = temporary;
                    second.validation.program = validationProgram;
                    second.specification = "Second approved feature.";
                    AssemblyLineDocument line = new AssemblyLineDocument();
                    AssemblyLineEntry firstEntry = AssemblyLineStore.Add(line, first);
                    AssemblyLineEntry secondEntry = AssemblyLineStore.Add(line, second);
                    Require(object.ReferenceEquals(AssemblyLineStore.FirstPending(line), firstEntry), "FIFO order changed");
                    string linePath = Path.Combine(temporary, "assembly-line.json");
                    AssemblyLineStore.Save(linePath, line);
                    AssemblyLineDocument loadedLine = AssemblyLineStore.Load(linePath);
                    Require(loadedLine.entries.Count == 2 && loadedLine.entries[1].id == secondEntry.id, "queue persistence changed entries");
                    loadedLine.entries[0].status = "Running";
                    Require(AssemblyLineStore.RecoverInterrupted(loadedLine) == 1, "running entry was not recovered as interrupted");
                    Require(loadedLine.entries[0].status == "Interrupted" && loadedLine.entries[1].status == "Pending", "recovery advanced another job");
                    Require(AssemblyLineStore.Retry(loadedLine.entries[0]), "explicit retry rejected");
                    Require(object.ReferenceEquals(AssemblyLineStore.FirstPending(loadedLine), loadedLine.entries[0]), "retry silently reordered FIFO");
                }
                finally
                {
                    try { Directory.Delete(temporary, true); } catch { }
                }
                passed.Add("assembly-line-fifo-persistence-recovery");

                string responseJson = "{\"id\":\"chat-1\",\"choices\":[{\"message\":{\"content\":\"Feature draft\"}}]}";
                ChatReply reply = LocalChatClient.ParseResponse(responseJson);
                Require(reply.Text == "Feature draft" && reply.RequestId == "chat-1", "chat response binding changed");
                bool rejectedEmpty = false;
                try { LocalChatClient.ParseResponse("{\"choices\":[{\"message\":{\"content\":\"\"}}]}"); }
                catch (InvalidDataException) { rejectedEmpty = true; }
                Require(rejectedEmpty, "empty chat response accepted");
                ChatHistoryDocument history = new ChatHistoryDocument();
                for (int index = 0; index < 40; index++)
                    ChatHistoryStore.Add(history, "owner", "message " + index, "model", "http://127.0.0.1:8080/v1", index.ToString());
                Require(history.messages.Count == ChatHistoryStore.MaximumMessages && history.messages[0].text == "message 8", "chat history bound changed");
                AssemblyLineDocument untouched = new AssemblyLineDocument();
                string copiedDraft = reply.Text;
                Require(copiedDraft == "Feature draft" && untouched.entries.Count == 0, "chat draft gained queue authority");
                passed.Add("bounded-chat-and-draft-authority");

                WriteResult(outputPath, true, passed, null);
                Console.WriteLine("{\"status\":\"passed\",\"tests\":" + passed.Count + "}");
                return 0;
            }
            catch (Exception exception)
            {
                WriteResult(outputPath, false, passed, exception.Message);
                Console.Error.WriteLine(exception);
                return 1;
            }
        }

        private static JobDocument ValidJob()
        {
            JobDocument job = new JobDocument();
            job.repository = "C:\\source";
            job.specification = "Implement the requested feature.";
            job.allowed_files.Add("src/main.cs");
            job.allowed_files.Add("tests/main.cs");
            job.validation.program = "C:\\tools\\test.exe";
            job.validation.args.Add("--filter");
            job.validation.args.Add("argument with spaces");
            return job;
        }

        private static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException("Self-test failed: " + message);
        }

        private static void WriteResult(string outputPath, bool success, List<string> passed, string error)
        {
            if (string.IsNullOrEmpty(outputPath)) return;
            string parent = Path.GetDirectoryName(outputPath);
            if (!string.IsNullOrEmpty(parent)) Directory.CreateDirectory(parent);
            Dictionary<string, object> result = new Dictionary<string, object>();
            result["status"] = success ? "passed" : "failed";
            result["passed"] = passed;
            result["error"] = error;
            File.WriteAllText(outputPath, new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(result), new UTF8Encoding(false));
        }
    }
}
