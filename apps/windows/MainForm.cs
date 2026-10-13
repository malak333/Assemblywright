using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace Assemblywright.Windows
{
    internal sealed class OwnedRunOutcome
    {
        internal int ExitCode;
        internal bool ConfirmedCompleted;
        internal PipelineReport Report;
        internal string Failure;
    }

    internal sealed class MainForm : Form
    {
        private const int MaximumLogBytes = 128 * 1024;
        private readonly string stateDirectory, initialJobPath, runnerPath, githubPath, demoScriptPath, screenshotPath;
        private readonly bool testMode;
        private readonly SemaphoreSlim commandGate;
        private readonly BoundedLog boundedLog;
        private readonly System.Windows.Forms.Timer statusTimer;
        private readonly ToolTip toolTips;
        private readonly StartOperationGate startGate;
        private readonly string settingsPath, draftPath, linePath, chatPath;
        private GuiSettings settings;
        private AssemblyLineDocument assemblyLine;
        private ChatHistoryDocument chatHistory;
        private string startupStateError;
        private bool assemblyLineUsable;

        private TextBox repositoryBox, specificationBox, allowedFilesBox, validationProgramBox, validationArgumentsBox, endpointBox, modelBox;
        private NumericUpDown timeoutBox, attemptsBox;
        private Button repositoryBrowseButton, validationBrowseButton, loadJobButton, saveJobButton, runFeatureButton, runDemoButton;
        private Button addToLineButton, startLineButton, stopLineButton, retryLineButton, removeLineButton, newFeatureButton, changeReviewerButton, reconcilePublicationButton;
        private Button cancelButton, pauseButton, resumeButton, candidateButton, reportButton;
        private HeaderIconButton settingsButton, safetyButton, githubButton;
        private Label statusLabel, stageLabel, ownershipLabel;
        private TextBox logBox, chatInputBox, chatTranscriptBox;
        private Button chatSendButton, chatStopButton, chatUseFeatureButton;
        private ListView assemblyLineList;
        private SplitContainer mainSplit;
        private TabControl leftTabs, rightTabs;
        private TabPage featureEditorTab, assemblyLineTab, chatTab, activityTab;

        private Process activeProcess;
        private TaskCompletionSource<bool> activeProcessCompletion;
        private PipelineReport latestReport;
        private string ownedRunId, runBaselineId, externalActiveId;
        private bool processStartedByThisGui, statusRequestActive, closingAfterCancel, allowClose;
        private bool queueLoopActive, queueStopRequested;
        private bool closeRequested;
        private AssemblyLineEntry activeLineEntry;
        private CancellationTokenSource chatCancellation;
        private bool approvalDialogOpen, githubOperationActive;
        private string editorLocalReasoningEffort, editorOrchestratorProvider, editorOrchestratorModel, editorOrchestratorReasoningEffort;
        private string editorReviewerProvider, editorReviewerModel, editorReviewerReasoningEffort;

        internal MainForm(string stateDirectory, string initialJobPath, bool testMode, string screenshotPath)
        {
            this.stateDirectory = Path.GetFullPath(stateDirectory);
            this.initialJobPath = initialJobPath;
            this.testMode = testMode;
            this.screenshotPath = screenshotPath;
            runnerPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "assemblywright-local.exe");
            githubPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "assemblywright-github.exe");
            demoScriptPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "windows-local-pipeline.ps1");
            settingsPath = Path.Combine(this.stateDirectory, "gui-settings.json");
            draftPath = Path.Combine(this.stateDirectory, "gui-job.json");
            linePath = Path.Combine(this.stateDirectory, "assembly-line.json");
            chatPath = Path.Combine(this.stateDirectory, "chat", "history.json");
            commandGate = new SemaphoreSlim(1, 1);
            boundedLog = new BoundedLog(MaximumLogBytes);
            statusTimer = new System.Windows.Forms.Timer();
            statusTimer.Interval = 900;
            statusTimer.Tick += async delegate { await RefreshStatusAsync(false); };
            toolTips = new ToolTip();
            startGate = new StartOperationGate();
            Directory.CreateDirectory(this.stateDirectory);
            LoadDurableGuiState();
            BuildWindow();
            Load += async delegate { await InitializeAsync(); };
            Shown += delegate
            {
                mainSplit.SplitterDistance = Math.Max(540, (int)(mainSplit.ClientSize.Width * 0.56));
                if (testMode) NativeMethods.ShowWindow(Handle, 4);
                ScheduleScreenshot();
            };
            FormClosing += OnFormClosing;
        }

        private void LoadDurableGuiState()
        {
            settings = new GuiSettings();
            assemblyLine = new AssemblyLineDocument();
            chatHistory = new ChatHistoryDocument();
            assemblyLineUsable = true;
            List<string> errors = new List<string>();
            try { settings = GuiSettingsStore.Load(settingsPath); }
            catch (Exception exception) { errors.Add(exception.Message); }
            try
            {
                assemblyLine = AssemblyLineStore.Load(linePath);
                int recovered = AssemblyLineStore.RecoverInterrupted(assemblyLine);
                if (recovered > 0) AssemblyLineStore.Save(linePath, assemblyLine);
            }
            catch (Exception exception)
            {
                assemblyLineUsable = false;
                errors.Add("Assembly Line state was not loaded: " + exception.Message);
            }
            try { chatHistory = ChatHistoryStore.Load(chatPath); }
            catch (Exception exception)
            {
                errors.Add("Chat history was not loaded: " + exception.Message);
                chatHistory = new ChatHistoryDocument();
            }
            startupStateError = string.Join(" ", errors.ToArray());
        }

        private void BuildWindow()
        {
            Text = "Assemblywright for Windows";
            Icon = SystemIcons.Application;
            Font = new Font("Segoe UI", 9F);
            BackColor = Color.FromArgb(244, 246, 250);
            MinimumSize = new Size(1100, 720);
            ClientSize = new Size(1280, 820);
            StartPosition = FormStartPosition.CenterScreen;
            if (testMode) { StartPosition = FormStartPosition.Manual; Location = new Point(-20000, -20000); ShowInTaskbar = false; }
            Panel header = BuildHeader();
            Controls.Add(header);
            mainSplit = new SplitContainer();
            mainSplit.Location = new Point(0, header.Height);
            mainSplit.Size = new Size(ClientSize.Width, ClientSize.Height - header.Height);
            mainSplit.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            mainSplit.Panel1.Padding = new Padding(16, 16, 8, 16);
            mainSplit.Panel2.Padding = new Padding(8, 16, 16, 16);
            mainSplit.BackColor = BackColor;
            Controls.Add(mainSplit);
            BuildLeftPane();
            BuildRightPane();
            PopulateSettingsDefaults();
            RefreshAssemblyLineList();
            RenderChatTranscript();
            SetInputsEnabled(true);
            UpdateControlButtons(null);
            UpdateProviderLocks();
        }

        private Panel BuildHeader()
        {
            Panel header = new Panel();
            header.Dock = DockStyle.Top;
            header.Size = new Size(ClientSize.Width, 74);
            header.BackColor = Color.FromArgb(27, 37, 54);
            Label title = new Label();
            title.Text = "ASSEMBLYWRIGHT";
            title.ForeColor = Color.White;
            title.Font = new Font("Segoe UI Semibold", 17F, FontStyle.Bold);
            title.AutoSize = true;
            title.Location = new Point(22, 13);
            Label subtitle = new Label();
            subtitle.Text = "Build approved features locally with your Windows AI";
            subtitle.ForeColor = Color.FromArgb(188, 204, 226);
            subtitle.AutoSize = true;
            subtitle.Location = new Point(24, 45);
            header.Controls.Add(title);
            header.Controls.Add(subtitle);
            githubButton = new HeaderIconButton(HeaderIconKind.GitHub, "GitHubButton", "GitHub publication");
            safetyButton = new HeaderIconButton(HeaderIconKind.Shield, "SafetyButton", "Permissions");
            settingsButton = new HeaderIconButton(HeaderIconKind.Gear, "SettingsButton", "Settings");
            githubButton.AccessibleDescription = "Open GitHub publication";
            safetyButton.AccessibleDescription = "Open global permissions";
            settingsButton.AccessibleDescription = "Choose local implementation and ChatGPT orchestrator and reviewer models";
            githubButton.Location = new Point(ClientSize.Width - 142, 17);
            safetyButton.Location = new Point(ClientSize.Width - 96, 17);
            settingsButton.Location = new Point(ClientSize.Width - 50, 17);
            settingsButton.Anchor = safetyButton.Anchor = githubButton.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            settingsButton.Click += OpenSettings;
            safetyButton.Click += OpenSafety;
            githubButton.Click += OpenGitHub;
            toolTips.SetToolTip(settingsButton, "ChatGPT roles and local implementation settings");
            header.Controls.Add(githubButton);
            toolTips.SetToolTip(safetyButton, "Permissions");
            toolTips.SetToolTip(githubButton, "GitHub publication");
            header.Controls.Add(safetyButton);
            header.Controls.Add(settingsButton);
            return header;
        }

        private void BuildLeftPane()
        {
            Panel card = CreateCard();
            card.Dock = DockStyle.Fill;
            mainSplit.Panel1.Controls.Add(card);
            leftTabs = new TabControl();
            leftTabs.Dock = DockStyle.Fill;
            leftTabs.Name = "LeftTabs";
            leftTabs.AccessibleName = "LeftTabs";
            card.Controls.Add(leftTabs);
            assemblyLineTab = new TabPage("Assembly Line");
            assemblyLineTab.Name = "AssemblyLineTab";
            assemblyLineTab.AccessibleName = "AssemblyLineTab";
            featureEditorTab = new TabPage("Feature");
            featureEditorTab.Name = "FeatureEditorTab";
            featureEditorTab.AccessibleName = "FeatureEditorTab";
            leftTabs.TabPages.Add(assemblyLineTab);
            leftTabs.TabPages.Add(featureEditorTab);
            BuildAssemblyLineTab();
            BuildFeatureEditorTab();
            leftTabs.SelectedTab = assemblyLineTab;
        }

        private void BuildAssemblyLineTab()
        {
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(12);
            layout.ColumnCount = 1;
            layout.RowCount = 4;
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 44));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 86));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 48));
            assemblyLineTab.Controls.Add(layout);
            Label heading = CreateFieldLabel("Approved features run one at a time in owner-approved FIFO order.");
            heading.Font = new Font("Segoe UI Semibold", 11F, FontStyle.Bold);
            layout.Controls.Add(heading);
            assemblyLineList = new ListView();
            assemblyLineList.Name = "AssemblyLineList";
            assemblyLineList.AccessibleName = "AssemblyLineList";
            assemblyLineList.Dock = DockStyle.Fill;
            assemblyLineList.View = View.Details;
            assemblyLineList.FullRowSelect = true;
            assemblyLineList.GridLines = true;
            assemblyLineList.HideSelection = false;
            assemblyLineList.Columns.Add("Feature", 190);
            assemblyLineList.Columns.Add("Repository", 150);
            assemblyLineList.Columns.Add("Status", 84);
            assemblyLineList.Columns.Add("Stage", 90);
            assemblyLineList.Columns.Add("Try", 44);
            assemblyLineList.Columns.Add("Result", 280);
            assemblyLineList.SelectedIndexChanged += delegate { UpdateLineButtons(); };
            layout.Controls.Add(assemblyLineList);
            FlowLayoutPanel actions = new FlowLayoutPanel();
            actions.Dock = DockStyle.Fill;
            newFeatureButton = CreateButton("New feature", "NewFeatureButton", NewFeature);
            startLineButton = CreatePrimaryButton("Start line", "StartLineButton", async delegate { await StartAssemblyLineAsync(); });
            stopLineButton = CreateDangerButton("Stop line", "StopLineButton", async delegate { await StopAssemblyLineAsync(); });
            retryLineButton = CreateButton("Retry selected", "RetryLineButton", RetrySelectedLine);
            removeLineButton = CreateButton("Remove selected", "RemoveLineButton", RemoveSelectedLine);
            changeReviewerButton = CreateButton("Change reviewer", "ChangeReviewerButton", async delegate { await ChangeSelectedReviewerAsync(); });
            reconcilePublicationButton = CreateButton("Reconcile", "ReconcilePublicationButton", async delegate { await ReconcileSelectedPublicationAsync(); });
            actions.Controls.Add(newFeatureButton);
            actions.Controls.Add(startLineButton);
            actions.Controls.Add(stopLineButton);
            actions.Controls.Add(retryLineButton);
            actions.Controls.Add(removeLineButton);
            actions.Controls.Add(changeReviewerButton);
            actions.Controls.Add(reconcilePublicationButton);
            layout.Controls.Add(actions);
            Label boundary = CreateFieldLabel("Unconnected projects keep a private candidate. Connected projects complete after the reviewed candidate is merged through GitHub required checks and the clean connected checkout is updated to that merge.");
            boundary.ForeColor = Color.FromArgb(88, 99, 115);
            layout.Controls.Add(boundary);
        }

        private void BuildFeatureEditorTab()
        {
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.ColumnCount = 1;
            layout.RowCount = 2;
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 94));
            featureEditorTab.Controls.Add(layout);
            TableLayoutPanel form = new TableLayoutPanel();
            form.Dock = DockStyle.Fill;
            form.Padding = new Padding(12);
            form.ColumnCount = 1;
            form.AutoScroll = true;
            layout.Controls.Add(form);
            AddSectionTitle(form, "Feature job");
            repositoryBox = CreateTextBox("RepositoryBox", false);
            repositoryBrowseButton = CreateButton("Browse...", "RepositoryBrowseButton", BrowseRepository);
            AddLabeledPicker(form, "Repository", repositoryBox, repositoryBrowseButton);
            specificationBox = CreateTextBox("SpecificationBox", true);
            AddLabeledControl(form, "Feature specification", specificationBox, 112);
            allowedFilesBox = CreateTextBox("AllowedFilesBox", true);
            AddLabeledControl(form, "Allowed files (one normalized relative path per line)", allowedFilesBox, 96);
            validationProgramBox = CreateTextBox("ValidationProgramBox", false);
            validationBrowseButton = CreateButton("Browse...", "ValidationBrowseButton", BrowseValidationProgram);
            AddLabeledPicker(form, "Validation program", validationProgramBox, validationBrowseButton);
            validationArgumentsBox = CreateTextBox("ValidationArgumentsBox", true);
            AddLabeledControl(form, "Validation arguments (one argument per line)", validationArgumentsBox, 88);
            TableLayoutPanel numbers = new TableLayoutPanel();
            numbers.Dock = DockStyle.Top;
            numbers.Height = 55;
            numbers.ColumnCount = 4;
            numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 34));
            numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 16));
            numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 34));
            numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 16));
            numbers.Controls.Add(CreateFieldLabel("Timeout seconds"), 0, 0);
            timeoutBox = CreateNumber("TimeoutBox", 1, 3600, settings.timeout_seconds);
            numbers.Controls.Add(timeoutBox, 1, 0);
            numbers.Controls.Add(CreateFieldLabel("Maximum attempts"), 2, 0);
            attemptsBox = CreateNumber("AttemptsBox", 1, 20, settings.default_attempts);
            numbers.Controls.Add(attemptsBox, 3, 0);
            AddRow(form, numbers, 58);
            endpointBox = CreateTextBox("EndpointBox", false);
            AddLabeledControl(form, "Local AI endpoint", endpointBox, 54);
            modelBox = CreateTextBox("ModelBox", false);
            AddLabeledControl(form, "Local model", modelBox, 54);
            Panel footer = new Panel();
            footer.Dock = DockStyle.Fill;
            footer.Padding = new Padding(12, 3, 12, 4);
            FlowLayoutPanel files = new FlowLayoutPanel();
            files.Dock = DockStyle.Top;
            files.Height = 40;
            loadJobButton = CreateButton("Load job", "LoadJobButton", LoadJobClicked);
            saveJobButton = CreateButton("Save job", "SaveJobButton", SaveJobClicked);
            files.Controls.Add(loadJobButton);
            files.Controls.Add(saveJobButton);
            FlowLayoutPanel execute = new FlowLayoutPanel();
            execute.Dock = DockStyle.Bottom;
            execute.Height = 42;
            addToLineButton = CreatePrimaryButton("Add to line", "AddToLineButton", async delegate { await AddCurrentFeatureToLineAsync(); });
            runFeatureButton = CreateButton("Run now", "RunFeatureButton", async delegate { await StartFeatureAsync(); });
            runDemoButton = CreateButton("Run demo", "RunDemoButton", async delegate { await StartDemoAsync(); });
            execute.Controls.Add(addToLineButton);
            execute.Controls.Add(runFeatureButton);
            execute.Controls.Add(runDemoButton);
            footer.Controls.Add(execute);
            footer.Controls.Add(files);
            layout.Controls.Add(footer);
        }

        private void BuildRightPane()
        {
            Panel card = CreateCard();
            card.Dock = DockStyle.Fill;
            mainSplit.Panel2.Controls.Add(card);
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(12);
            layout.ColumnCount = 1;
            layout.RowCount = 3;
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 178));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 46));
            card.Controls.Add(layout);
            TableLayoutPanel compact = new TableLayoutPanel();
            compact.Dock = DockStyle.Fill;
            compact.ColumnCount = 1;
            compact.RowCount = 5;
            compact.RowStyles.Add(new RowStyle(SizeType.Absolute, 33));
            compact.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
            compact.RowStyles.Add(new RowStyle(SizeType.Absolute, 26));
            compact.RowStyles.Add(new RowStyle(SizeType.Absolute, 30));
            compact.RowStyles.Add(new RowStyle(SizeType.Absolute, 47));
            layout.Controls.Add(compact);
            Label heading = CreateFieldLabel("Pipeline status");
            heading.Font = new Font("Segoe UI Semibold", 12F, FontStyle.Bold);
            compact.Controls.Add(heading);
            statusLabel = CreateStatusLabel("StatusLabel", "Ready");
            statusLabel.Font = new Font("Segoe UI Semibold", 14F, FontStyle.Bold);
            compact.Controls.Add(statusLabel);
            stageLabel = CreateStatusLabel("StageLabel", "Stage: idle");
            compact.Controls.Add(stageLabel);
            ownershipLabel = CreateStatusLabel("OwnershipLabel", "No active job");
            ownershipLabel.ForeColor = Color.FromArgb(88, 99, 115);
            compact.Controls.Add(ownershipLabel);
            FlowLayoutPanel controls = new FlowLayoutPanel();
            controls.Dock = DockStyle.Fill;
            pauseButton = CreateButton("Pause", "PauseButton", async delegate { await PauseAllAsync(); });
            resumeButton = CreateButton("Resume", "ResumeButton", async delegate { await InvokeControlAsync("resume"); });
            cancelButton = CreateDangerButton("Cancel", "CancelButton", async delegate { await CancelPipelineAsync(); });
            controls.Controls.Add(pauseButton);
            controls.Controls.Add(resumeButton);
            controls.Controls.Add(cancelButton);
            compact.Controls.Add(controls);
            rightTabs = new TabControl();
            rightTabs.Dock = DockStyle.Fill;
            rightTabs.Name = "RightTabs";
            chatTab = new TabPage("Chat");
            chatTab.Name = "ChatTab";
            chatTab.AccessibleName = "ChatTab";
            activityTab = new TabPage("Activity");
            activityTab.Name = "ActivityTab";
            activityTab.AccessibleName = "ActivityTab";
            rightTabs.TabPages.Add(chatTab);
            rightTabs.TabPages.Add(activityTab);
            rightTabs.SelectedTab = chatTab;
            layout.Controls.Add(rightTabs);
            BuildChatTab();
            logBox = CreateTextBox("LogBox", true);
            logBox.ReadOnly = true;
            logBox.BackColor = Color.FromArgb(17, 24, 36);
            logBox.ForeColor = Color.FromArgb(220, 231, 244);
            logBox.Font = new Font("Consolas", 9F);
            activityTab.Controls.Add(logBox);
            FlowLayoutPanel results = new FlowLayoutPanel();
            results.Dock = DockStyle.Fill;
            candidateButton = CreateButton("Open candidate", "CandidateButton", OpenCandidate);
            reportButton = CreateButton("Open report", "ReportButton", OpenReport);
            candidateButton.Enabled = false;
            reportButton.Enabled = false;
            results.Controls.Add(candidateButton);
            results.Controls.Add(reportButton);
            layout.Controls.Add(results);
        }

        private void BuildChatTab()
        {
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(8);
            layout.ColumnCount = 1;
            layout.RowCount = 3;
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 86));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 42));
            chatTab.Controls.Add(layout);
            chatTranscriptBox = CreateTextBox("ChatTranscriptBox", true);
            chatTranscriptBox.ReadOnly = true;
            chatTranscriptBox.BackColor = Color.White;
            layout.Controls.Add(chatTranscriptBox);
            chatInputBox = CreateTextBox("ChatInputBox", true);
            layout.Controls.Add(chatInputBox);
            FlowLayoutPanel actions = new FlowLayoutPanel();
            actions.Dock = DockStyle.Fill;
            chatSendButton = CreatePrimaryButton("Send", "ChatSendButton", async delegate { await SendChatAsync(); });
            chatStopButton = CreateDangerButton("Stop", "ChatStopButton", delegate { CancelChat(); });
            chatUseFeatureButton = CreateButton("Use as feature", "ChatUseFeatureButton", UseChatAsFeature);
            actions.Controls.Add(chatSendButton);
            actions.Controls.Add(chatStopButton);
            actions.Controls.Add(chatUseFeatureButton);
            layout.Controls.Add(actions);
        }

        private void PopulateSettingsDefaults()
        {
            endpointBox.Text = settings.endpoint;
            modelBox.Text = settings.model;
            editorReviewerModel = settings.reviewer_model;
            editorLocalReasoningEffort = settings.reasoning_effort;
            editorOrchestratorProvider = settings.orchestrator_provider;
            editorOrchestratorModel = settings.orchestrator_model;
            editorOrchestratorReasoningEffort = settings.orchestrator_reasoning_effort;
            editorReviewerProvider = settings.reviewer_provider;
            editorReviewerReasoningEffort = settings.reviewer_reasoning_effort;
            attemptsBox.Value = Clamp(settings.default_attempts, 1, 20);
            timeoutBox.Value = Clamp(settings.timeout_seconds, 1, 3600);
        }

        private async Task InitializeAsync()
        {
            if (!File.Exists(runnerPath)) { SurfaceError("The packaged local runner is missing: " + runnerPath); SetInputsEnabled(false); return; }
            if (!string.IsNullOrEmpty(initialJobPath))
            {
                try { LoadJob(initialJobPath, true); leftTabs.SelectedTab = featureEditorTab; }
                catch (Exception exception) { SurfaceError("Could not load the requested job: " + exception.Message); }
            }
            else if (File.Exists(draftPath))
            {
                try { PopulateForm(JobJson.DeserializeJob(SharedFile.ReadUtf8Json(draftPath))); }
                catch (Exception exception) { AppendLog("Could not restore the job draft: " + exception.Message + Environment.NewLine); }
            }
            await EnsureAdvertisedChatGptDefaultsAsync(string.IsNullOrEmpty(initialJobPath) && !File.Exists(draftPath));
            if (string.IsNullOrEmpty(initialJobPath) && File.Exists(draftPath)) ApplyRoleSettingsToLegacyDraft();
            if (!string.IsNullOrWhiteSpace(startupStateError)) SurfaceError(startupStateError);
            await RefreshStatusAsync(true);
            statusTimer.Start();
        }

        private void ApplyRoleSettingsToLegacyDraft()
        {
            if (string.IsNullOrWhiteSpace(editorOrchestratorProvider))
            {
                editorOrchestratorProvider = settings.orchestrator_provider; editorOrchestratorModel = settings.orchestrator_model; editorOrchestratorReasoningEffort = settings.orchestrator_reasoning_effort;
            }
            if (string.IsNullOrWhiteSpace(editorReviewerProvider))
            {
                editorReviewerProvider = settings.reviewer_provider; editorReviewerModel = settings.reviewer_model; editorReviewerReasoningEffort = settings.reviewer_reasoning_effort;
            }
        }

        private async Task EnsureAdvertisedChatGptDefaultsAsync(bool applyToEditor)
        {
            if (settings.orchestrator_provider != RoleProvider.ChatGpt && settings.reviewer_provider != RoleProvider.ChatGpt) return;
            try
            {
                List<ChatGptModelOption> catalog = await ChatGptModelCatalog.LoadAsync(runnerPath, stateDirectory);
                ChatGptModelOption first = catalog.First();
                if (settings.orchestrator_provider == RoleProvider.ChatGpt && string.IsNullOrWhiteSpace(settings.orchestrator_model))
                { settings.orchestrator_model = first.Id; settings.orchestrator_reasoning_effort = first.DefaultReasoningEffort; }
                if (settings.reviewer_provider == RoleProvider.ChatGpt && string.IsNullOrWhiteSpace(settings.reviewer_model))
                { settings.reviewer_model = first.Id; settings.reviewer_reasoning_effort = first.DefaultReasoningEffort; }
                if (applyToEditor)
                {
                    editorOrchestratorProvider = settings.orchestrator_provider; editorOrchestratorModel = settings.orchestrator_model; editorOrchestratorReasoningEffort = settings.orchestrator_reasoning_effort;
                    editorReviewerProvider = settings.reviewer_provider; editorReviewerModel = settings.reviewer_model; editorReviewerReasoningEffort = settings.reviewer_reasoning_effort;
                }
            }
            catch (Exception exception) { AppendLog("ChatGPT model catalog: " + exception.Message + Environment.NewLine); }
        }

        private async Task AddCurrentFeatureToLineAsync()
        {
            if (!assemblyLineUsable) { SurfaceError("The Assembly Line is unavailable until its invalid state file is repaired or removed by the owner."); return; }
            JobDocument job = ReadJobFromForm();
            string validation = JobValidator.ValidateForForm(job, true);
            if (validation != null) { SurfaceError(validation); return; }
            if (!await CheckCleanRepositoryAsync(job.repository)) return;
            try
            {
                AssemblyLineStore.Add(assemblyLine, job);
                PersistAssemblyLine();
                RefreshAssemblyLineList();
                SaveDraft();
                leftTabs.SelectedTab = assemblyLineTab;
                statusLabel.Text = "Added to line";
                ownershipLabel.Text = "The frozen feature is waiting for the owner to start the line.";
            }
            catch (Exception exception) { SurfaceError("Could not add the feature: " + exception.Message); }
        }

        private async Task StartAssemblyLineAsync()
        {
            if (!TryBeginStartOperation()) return;
            try
            {
                if (!assemblyLineUsable || queueLoopActive || activeProcess != null) return;
                queueStopRequested = false;
                await RefreshStatusAsync(true);
                if (!PrelaunchAuthority.MayLaunch(queueStopRequested, closeRequested)) return;
                if (latestReport != null && latestReport.status == "running") { SurfaceError("Another local pipeline is active."); return; }
                if (latestReport != null && latestReport.paused) { SurfaceError("Local Emergency Pause is active. Resume before starting the line."); return; }
                if (AssemblyLineStore.FirstPending(assemblyLine) == null) { SurfaceError("The Assembly Line has no pending feature. Add or explicitly retry one first."); return; }
                queueLoopActive = true;
                queueStopRequested = false;
                UpdateLineButtons();
                while (!queueStopRequested)
                {
                    AssemblyLineEntry entry = AssemblyLineStore.FirstPending(assemblyLine);
                    if (entry == null) break;
                    activeLineEntry = entry;
                    entry.status = "Running";
                    entry.stage = "starting";
                    entry.result = "Starting the owner-approved frozen job.";
                    entry.updated_at_ms = AssemblyLineStore.NowMilliseconds();
                    PersistAssemblyLine();
                    RefreshAssemblyLineList();
                    JobDocument job = JobJson.DeserializeJob(entry.frozen_job_json);
                    string validation = JobValidator.ValidateForForm(job, true);
                    if (validation != null) { MarkLineTerminal(entry, "Failed", "Frozen job is no longer valid: " + validation, null); queueStopRequested = true; break; }
                    bool clean = await CheckCleanRepositoryAsync(job.repository);
                    if (!PrelaunchAuthority.MayLaunch(queueStopRequested, closeRequested))
                    {
                        MarkLineTerminal(entry, "Cancelled", "Stopped before the native runner was launched.", null);
                        break;
                    }
                    if (!clean) { MarkLineTerminal(entry, "Failed", "Source repository preflight failed. Review the activity log.", null); queueStopRequested = true; break; }
                    OwnedRunOutcome outcome = await StartJobDocumentAsync(job, entry.frozen_job_json);
                    if (outcome.ConfirmedCompleted && !queueStopRequested)
                    {
                        bool publicationConfirmed = await PublishIfConnectedAsync(entry, outcome.Report);
                        if (!publicationConfirmed)
                        {
                            MarkLineTerminal(entry, queueStopRequested ? "Cancelled" : "Failed", "GitHub publication did not reach a confirmed merged result. Review the retained publication before retrying or reconciling.", outcome.Report);
                            queueStopRequested = true;
                            break;
                        }
                        string completedMessage = entry.publication_status == "succeeded" ? "Validated, reviewed, merged on GitHub, and the clean connected checkout was updated to the merge." : "Validated private candidate ready. This project is not connected to GitHub.";
                        MarkLineTerminal(entry, "Completed", completedMessage, outcome.Report);
                        activeLineEntry = null;
                        continue;
                    }
                    string terminal = outcome.Report != null && outcome.Report.status == "cancelled" ? "Cancelled" : "Failed";
                    if (queueStopRequested && terminal == "Failed") terminal = "Cancelled";
                    MarkLineTerminal(entry, terminal, string.IsNullOrWhiteSpace(outcome.Failure) ? "The line halted before confirmed completion." : outcome.Failure, outcome.Report);
                    queueStopRequested = true;
                    break;
                }
            }
            catch (Exception exception)
            {
                if (activeLineEntry != null) MarkLineTerminal(activeLineEntry, "Failed", exception.Message, latestReport);
                SurfaceError("The Assembly Line halted: " + exception.Message);
            }
            finally
            {
                activeLineEntry = null;
                queueLoopActive = false;
                queueStopRequested = false;
                Exception presentationError = AssemblyLineTransition.TryProject(delegate
                {
                    EndStartOperation();
                    if (!IsDisposed) { RefreshAssemblyLineList(); UpdateLineButtons(); }
                });
                if (presentationError != null) HandleLinePresentationFailure(presentationError);
            }
        }

        private async Task StopAssemblyLineAsync()
        {
            queueStopRequested = true;
            CancelChat();
            if (githubOperationActive) await CancelGitHubOperationAsync();
            else if (activeProcess != null) await InvokeControlAsync("cancel");
            else { statusLabel.Text = "Line stopped"; ownershipLabel.Text = "Pending features remain frozen. Start line explicitly when ready."; }
            UpdateLineButtons();
        }

        private async Task<bool> PublishIfConnectedAsync(AssemblyLineEntry entry, PipelineReport report)
        {
            if (!File.Exists(githubPath)) { entry.publication_status = "attention"; entry.result = "The packaged GitHub authority is missing, so Assemblywright cannot prove whether this project requires publication."; PersistAssemblyLine(); SurfaceError(entry.result); return false; }
            GitHubResponse connection;
            try { connection = await GitHubNative.RunAsync(githubPath, "connection", "get", "--state-dir", stateDirectory, "--project", Path.GetFullPath(entry.repository)); }
            catch (Exception exception) { SurfaceError("Could not determine GitHub connection: " + exception.Message); return false; }
            if (queueStopRequested || closeRequested) { entry.publication_status = "cancelled"; PersistAssemblyLine(); return false; }
            if (connection.connection == null)
            {
                entry.publication_status = "local_only"; entry.publication_url = string.Empty; entry.publication_merge_sha = string.Empty; PersistAssemblyLine(); return true;
            }
            githubOperationActive = true; entry.publication_status = "running"; entry.stage = "publication"; entry.result = "Publishing the exact reviewed candidate through a pull request and required checks."; PersistAssemblyLine(); RefreshAssemblyLineList(); UpdateProviderLocks();
            try
            {
                GitHubResponse result = await GitHubNative.RunAsync(githubPath, "publish", "--state-dir", stateDirectory, "--run-id", report.run_id, "--expected-revision", connection.revision.ToString(System.Globalization.CultureInfo.InvariantCulture));
                GitHubResponse durable = await GitHubNative.RunAsync(githubPath, "status", "--state-dir", stateDirectory);
                GitHubPublication persisted = null; bool retained = durable.publications != null && durable.publications.TryGetValue(report.run_id, out persisted);
                if (result.status != "succeeded" || result.revision <= connection.revision || durable.revision != result.revision || !retained
                    || !GitHubNative.SamePublication(result.publication, persisted) || !GitHubNative.PublicationMatches(persisted, report, connection.connection)
                    || !GitHubNative.DurableReceiptMatches(durable, report.run_id, persisted))
                    throw new InvalidDataException("Durable GitHub state did not confirm a publication bound to this exact connection revision, source head, run, candidate, project, repository, base branch, feature branch, pull request, and merge result.");
                entry.publication_status = "succeeded"; entry.publication_url = persisted.pull_request_url; entry.publication_merge_sha = persisted.merge_commit; entry.result = persisted.message ?? "Merged on GitHub"; PersistAssemblyLine(); return true;
            }
            catch (Exception exception)
            {
                entry.publication_status = queueStopRequested ? "cancelled" : "attention"; entry.result = exception.Message; PersistAssemblyLine(); SurfaceError("GitHub publication needs attention: " + exception.Message); return false;
            }
            finally { githubOperationActive = false; UpdateProviderLocks(); }
        }

        private async Task CancelGitHubOperationAsync()
        {
            try { await GitHubNative.RunAsync(githubPath, "cancel", "--state-dir", stateDirectory); statusLabel.Text = "Cancelling GitHub operation"; }
            catch (Exception exception) { SurfaceError("GitHub cancellation failed: " + exception.Message); }
        }

        private void MarkLineTerminal(AssemblyLineEntry entry, string status, string result, PipelineReport report)
        {
            entry.status = status;
            entry.stage = report == null || string.IsNullOrWhiteSpace(report.stage) ? status.ToLowerInvariant() : report.stage;
            entry.attempt = report == null ? entry.attempt : report.attempt;
            entry.result = result;
            entry.run_id = report == null ? entry.run_id : (report.run_id ?? string.Empty);
            entry.updated_at_ms = AssemblyLineStore.NowMilliseconds();
            Exception presentationError = AssemblyLineTransition.PersistThenProject(PersistAssemblyLine, delegate
            {
                if (!IsDisposed) RefreshAssemblyLineList();
            });
            if (presentationError != null) HandleLinePresentationFailure(presentationError);
        }

        private void HandleLinePresentationFailure(Exception exception)
        {
            queueStopRequested = true;
            if (IsDisposed) return;
            string message = "The Assembly Line display could not refresh. Saved feature results are retained; the line is stopped. " + exception.Message;
            try { SurfaceError(message); }
            catch (Exception) { try { AppendLog(message + Environment.NewLine); } catch (Exception) { } }
        }

        private async Task StartFeatureAsync()
        {
            if (!TryBeginStartOperation()) return;
            try
            {
                if (activeProcess != null || queueLoopActive) return;
                queueStopRequested = false;
                JobDocument job = ReadJobFromForm();
                string validation = JobValidator.ValidateForForm(job, true);
                if (validation != null) { SurfaceError(validation); return; }
                if (!await CheckCleanRepositoryAsync(job.repository)) return;
                if (!PrelaunchAuthority.MayLaunch(queueStopRequested, closeRequested)) return;
                await StartJobDocumentAsync(job, JobJson.Serialize(job));
            }
            finally { EndStartOperation(); }
        }

        private async Task<OwnedRunOutcome> StartJobDocumentAsync(JobDocument job, string exactJson)
        {
            await RefreshStatusAsync(true);
            if (!PrelaunchAuthority.MayLaunch(queueStopRequested, closeRequested)) return FailedOutcome("Stopped before the native runner was launched.");
            if (latestReport != null && latestReport.status == "running") { SurfaceError("Another local pipeline owns this state directory. Cancel it or wait for it to finish."); return FailedOutcome("Another local pipeline is active."); }
            if (latestReport != null && latestReport.paused) { SurfaceError("Local Emergency Pause is active. Resume before starting a feature."); return FailedOutcome("Local Emergency Pause is active."); }
            string frozenJob = Path.Combine(stateDirectory, "jobs", DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ") + "-" + Guid.NewGuid().ToString("N") + ".json");
            JobJson.WriteUtf8(frozenJob, exactJson);
            AppendLog("Frozen job: " + frozenJob + Environment.NewLine);
            return await StartOwnedProcessAsync(runnerPath, new string[] { "run", "--job", frozenJob, "--state-dir", stateDirectory });
        }

        private async Task StartDemoAsync()
        {
            if (!TryBeginStartOperation()) return;
            try
            {
                if (activeProcess != null || queueLoopActive) return;
                queueStopRequested = false;
                if (!File.Exists(demoScriptPath)) { SurfaceError("The packaged demo launcher is missing: " + demoScriptPath); return; }
                await RefreshStatusAsync(true);
                if (!PrelaunchAuthority.MayLaunch(queueStopRequested, closeRequested)) return;
                if (latestReport != null && latestReport.status == "running") { SurfaceError("Another local pipeline is active."); return; }
                if (latestReport != null && latestReport.paused) { SurfaceError("Local Emergency Pause is active. Resume before starting the demo."); return; }
                JobDocument demoSettings = DemoJobSettings.CreateSnapshot(settings, AppDomain.CurrentDomain.BaseDirectory, demoScriptPath);
                string frozenSettings = Path.Combine(stateDirectory, "jobs", DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ") + "-demo-settings-" + Guid.NewGuid().ToString("N") + ".json");
                JobJson.WriteUtf8(frozenSettings, JobJson.Serialize(demoSettings));
                AppendLog("Frozen demo settings: " + frozenSettings + Environment.NewLine);
                await StartOwnedProcessAsync("powershell.exe", new string[] { "-NoProfile", "-File", demoScriptPath, "-Demo", "-DemoSettingsJob", frozenSettings, "-NoBuild", "-RunnerPath", runnerPath, "-StateDir", stateDirectory });
            }
            finally { EndStartOperation(); }
        }

        private async Task<OwnedRunOutcome> StartOwnedProcessAsync(string executable, IEnumerable<string> arguments)
        {
            OwnedRunOutcome outcome = new OwnedRunOutcome();
            BoundedCapture standardOutput = new BoundedCapture(2 * 1024 * 1024);
            TaskCompletionSource<bool> processCompletion = new TaskCompletionSource<bool>();
            Process launchedProcess = null;
            bool lifecycleConfirmed = false;
            activeProcessCompletion = processCompletion;
            outcome.ExitCode = -1;
            runBaselineId = latestReport == null ? null : latestReport.run_id;
            ownedRunId = null;
            processStartedByThisGui = true;
            candidateButton.Enabled = false;
            reportButton.Enabled = false;
            SetInputsEnabled(false);
            UpdateProviderLocks();
            statusLabel.Text = "Starting";
            ownershipLabel.Text = "Current job submitted by this window";
            try
            {
                launchedProcess = ProcessRunner.StartStreaming(executable, arguments,
                    delegate(string value) { standardOutput.Append(value); AppendLog(value); }, AppendLog);
                activeProcess = launchedProcess;
                AppendLog("Started process " + launchedProcess.Id + "." + Environment.NewLine);
                await Task.Run(delegate { launchedProcess.WaitForExit(); });
                outcome.ExitCode = launchedProcess.ExitCode;
                launchedProcess.Dispose();
                activeProcess = null;
                lifecycleConfirmed = true;
                processCompletion.TrySetResult(true);
            }
            catch (Exception exception)
            {
                ProcessRunner.ProcessLaunchException launchException = exception as ProcessRunner.ProcessLaunchException;
                if (launchException != null && launchException.ActiveProcess != null)
                    launchedProcess = launchException.ActiveProcess;
                lifecycleConfirmed = ProcessRunner.TryTerminateAndDispose(launchedProcess, 5000);
                if (lifecycleConfirmed) activeProcess = null;
                else activeProcess = launchedProcess;
                processCompletion.TrySetResult(lifecycleConfirmed);
                outcome.Failure = exception.Message;
                SurfaceError("Could not run the pipeline: " + exception.Message);
            }

            if (lifecycleConfirmed && outcome.ExitCode >= 0)
            {
                PipelineReport childReport;
                if (standardOutput.Overflowed || !ReportBindings.TryExtractTerminalReport(standardOutput.Text, out childReport))
                {
                    childReport = null;
                    outcome.Failure = standardOutput.Overflowed
                        ? "The child report output exceeded its 2 MiB ownership boundary."
                        : "The child did not emit one exact terminal report for ownership binding.";
                }
                else
                {
                    ownedRunId = childReport.run_id;
                }
                await RefreshStatusAsync(true);
                outcome.Report = latestReport;
                bool newRun = latestReport != null && !string.IsNullOrWhiteSpace(latestReport.run_id) && !string.Equals(runBaselineId, latestReport.run_id, StringComparison.Ordinal);
                outcome.ConfirmedCompleted = RunCompletionAuthority.IsConfirmed(outcome.ExitCode, childReport, latestReport, runBaselineId);
                if (!outcome.ConfirmedCompleted)
                {
                    if (string.IsNullOrWhiteSpace(outcome.Failure))
                        outcome.Failure = latestReport != null && newRun && !string.IsNullOrWhiteSpace(latestReport.message) ? latestReport.message : "The pipeline exited without a new owned completed report (exit " + outcome.ExitCode + ").";
                    if (outcome.ExitCode != 0 && (latestReport == null || latestReport.status == "running")) SurfaceError("The pipeline exited with code " + outcome.ExitCode + ". See the activity log.");
                }
            }
            processStartedByThisGui = activeProcess != null;
            SetInputsEnabled(activeProcess == null);
            UpdateProviderLocks();
            await RefreshStatusAsync(true);
            return outcome;
        }

        private static OwnedRunOutcome FailedOutcome(string message)
        {
            OwnedRunOutcome outcome = new OwnedRunOutcome(); outcome.ExitCode = -1; outcome.Failure = message; return outcome;
        }

        private async Task<bool> CheckCleanRepositoryAsync(string repository)
        {
            statusLabel.Text = "Checking repository";
            try
            {
                ProcessResult head = await ProcessRunner.RunCapturedAsync("git.exe", new string[] { "-C", repository, "rev-parse", "--verify", "HEAD" });
                if (head.ExitCode != 0) { SurfaceError("The repository has no committed HEAD. " + FirstUsefulLine(head.StandardError)); return false; }
                ProcessResult status = await ProcessRunner.RunCapturedAsync("git.exe", new string[] { "-C", repository, "status", "--porcelain=v1", "--untracked-files=all" });
                if (status.ExitCode != 0) { SurfaceError("Git could not inspect the repository. " + FirstUsefulLine(status.StandardError)); return false; }
                if (!string.IsNullOrWhiteSpace(status.StandardOutput))
                {
                    SurfaceError("The source repository must be clean before Assemblywright can create a candidate. Commit, stash, or remove these changes:" + Environment.NewLine + status.StandardOutput.Trim());
                    return false;
                }
                return true;
            }
            catch (Exception exception) { SurfaceError("Could not verify the source repository: " + exception.Message); return false; }
        }

        private async Task RefreshStatusAsync(bool force)
        {
            if (statusRequestActive && !force) return;
            if (!File.Exists(runnerPath)) return;
            statusRequestActive = true;
            await commandGate.WaitAsync();
            try
            {
                ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { "status", "--state-dir", stateDirectory });
                if (result.ExitCode != 0) { if (force) SurfaceError("Could not refresh pipeline status. " + FirstUsefulLine(result.StandardError)); return; }
                PipelineReport report = JobJson.DeserializeReport(result.StandardOutput);
                latestReport = report;
                if (!processStartedByThisGui && string.IsNullOrEmpty(ownedRunId) && report.status == "running") externalActiveId = report.run_id;
                ApplyReport(report);
            }
            catch (Exception exception) { if (force) SurfaceError("Could not refresh pipeline status: " + exception.Message); }
            finally { commandGate.Release(); statusRequestActive = false; }
            if (activeProcess != null && !approvalDialogOpen) await SurfacePendingApprovalAsync();
        }

        private async Task SurfacePendingApprovalAsync()
        {
            ApprovalDocument approval;
            try { approval = await NativeGuiControl.GetApprovalAsync(runnerPath, stateDirectory); }
            catch (Exception exception) { AppendLog("Approval status: " + exception.Message + Environment.NewLine); return; }
            if (approval.status != "pending" || approvalDialogOpen) return;
            approvalDialogOpen = true;
            try
            {
                using (ApprovalDialog dialog = new ApprovalDialog(approval))
                {
                    DialogResult result = dialog.ShowDialog(this);
                    bool approve = result == DialogResult.Yes;
                    await NativeGuiControl.DecideApprovalAsync(runnerPath, stateDirectory, approval, approve);
                    AppendLog((approve ? "Approved " : "Rejected ") + approval.action + " request " + approval.id + "." + Environment.NewLine);
                }
            }
            catch (Exception exception) { SurfaceError("The approval decision failed: " + exception.Message); }
            finally { approvalDialogOpen = false; }
        }

        private void ApplyReport(PipelineReport report)
        {
            ReportBindingKind kind = ReportBindings.Classify(report, ownedRunId, processStartedByThisGui);
            statusLabel.Text = DisplayStatus(report.status, report.paused);
            stageLabel.Text = "Stage: " + (string.IsNullOrEmpty(report.stage) ? "idle" : report.stage) + (report.attempt > 0 ? "  |  Attempt " + report.attempt : string.Empty);
            if (!string.IsNullOrWhiteSpace(report.message)) ownershipLabel.Text = report.message;
            if (kind == ReportBindingKind.OwnRunning) ownershipLabel.Text = "Current job from this window - " + report.message;
            else if (kind == ReportBindingKind.OwnCompleted) ownershipLabel.Text = "Current job completed - private candidate is ready";
            else if (kind == ReportBindingKind.OwnTerminal) ownershipLabel.Text = "Current job ended - " + report.message;
            else if (kind == ReportBindingKind.ExternalRunning) ownershipLabel.Text = "A pipeline started outside this window is active";
            else if (!string.IsNullOrEmpty(externalActiveId) && externalActiveId == report.run_id) ownershipLabel.Text = "Pipeline started outside this window ended - " + report.message;
            else if (kind == ReportBindingKind.PreviousCompleted) ownershipLabel.Text = "Previous completed result";
            else if (kind == ReportBindingKind.None) ownershipLabel.Text = report.paused ? "Local Emergency Pause is active" : "No active job";
            bool completed = report.status == "completed" && !string.IsNullOrWhiteSpace(report.run_id);
            candidateButton.Enabled = completed && !string.IsNullOrWhiteSpace(report.candidate) && Directory.Exists(UsableWindowsPath(report.candidate));
            reportButton.Enabled = completed && File.Exists(ReportPath(report));
            if (report.paused) { queueStopRequested = true; CancelChat(); }
            if (activeLineEntry != null && report.status == "running")
            {
                bool changed = activeLineEntry.stage != report.stage || activeLineEntry.attempt != report.attempt;
                activeLineEntry.stage = string.IsNullOrWhiteSpace(report.stage) ? "running" : report.stage;
                activeLineEntry.attempt = report.attempt;
                activeLineEntry.run_id = report.run_id ?? string.Empty;
                if (changed) { activeLineEntry.updated_at_ms = AssemblyLineStore.NowMilliseconds(); PersistAssemblyLine(); RefreshAssemblyLineList(); }
            }
            UpdateControlButtons(report);
            UpdateLineButtons();
        }

        private async Task<bool> InvokeControlAsync(string action)
        {
            bool success = false;
            await commandGate.WaitAsync();
            SetControlButtonsEnabled(false);
            try
            {
                ProcessResult result = await ProcessRunner.RunCapturedAsync(runnerPath, new string[] { action, "--state-dir", stateDirectory });
                if (result.ExitCode != 0) SurfaceError("The " + action + " command failed. " + FirstUsefulLine(result.StandardError));
                else { success = true; AppendLog(result.StandardOutput); statusLabel.Text = action == "resume" ? "Resumed" : (action == "pause" ? "Pausing" : "Cancelling"); }
            }
            catch (Exception exception) { SurfaceError("The " + action + " command failed: " + exception.Message); }
            finally { commandGate.Release(); }
            await RefreshStatusAsync(true);
            return success;
        }

        private async Task<bool> PauseAllAsync() { queueStopRequested = true; CancelChat(); return await InvokeControlAsync("pause"); }
        private async Task<bool> CancelPipelineAsync()
        {
            queueStopRequested = true;
            if (githubOperationActive) { await CancelGitHubOperationAsync(); return true; }
            return await InvokeControlAsync("cancel");
        }

        private async Task SendChatAsync()
        {
            if (chatCancellation != null) return;
            if (latestReport != null && latestReport.paused) { SurfaceError("Local Emergency Pause blocks new chat generation until the owner resumes."); return; }
            string ownerText = chatInputBox.Text.Trim();
            if (ownerText.Length == 0) return;
            string provider = settings.orchestrator_provider, model = settings.orchestrator_model, reasoningEffort = settings.orchestrator_reasoning_effort;
            string endpoint = provider == RoleProvider.Local ? settings.endpoint : string.Empty;
            List<ChatHistoryMessage> prior = new List<ChatHistoryMessage>(chatHistory.messages);
            CancellationTokenSource cancellation = new CancellationTokenSource();
            chatCancellation = cancellation;
            ChatHistoryStore.Add(chatHistory, "owner", ownerText, model, endpoint, Guid.NewGuid().ToString("D"), provider, reasoningEffort);
            PersistChatHistory();
            chatInputBox.Clear();
            RenderChatTranscript();
            UpdateChatButtons();
            UpdateProviderLocks();
            try
            {
                ChatReply reply = provider == RoleProvider.ChatGpt
                    ? await ChatGptChatClient.SendAsync(runnerPath, stateDirectory, model, reasoningEffort, prior, specificationBox.Text, ownerText, cancellation.Token)
                    : await LocalChatClient.SendAsync(endpoint, model, prior, specificationBox.Text, ownerText, cancellation.Token);
                if (cancellation.IsCancellationRequested) return;
                ChatHistoryStore.Add(chatHistory, "assistant", reply.Text, model, endpoint, reply.RequestId, provider, reasoningEffort);
                PersistChatHistory();
                RenderChatTranscript();
            }
            catch (OperationCanceledException)
            {
                ChatHistoryStore.Add(chatHistory, "error", "Chat request stopped by the owner or Emergency Pause.", model, endpoint, string.Empty, provider, reasoningEffort);
                PersistChatHistory();
                RenderChatTranscript();
            }
            catch (Exception exception)
            {
                ChatHistoryStore.Add(chatHistory, "error", exception.Message, model, endpoint, string.Empty, provider, reasoningEffort);
                PersistChatHistory();
                RenderChatTranscript();
                SurfaceError(RoleProvider.DisplayName(provider) + " chat failed: " + exception.Message);
            }
            finally
            {
                if (ReferenceEquals(chatCancellation, cancellation)) chatCancellation = null;
                cancellation.Dispose();
                UpdateChatButtons();
                UpdateProviderLocks();
            }
        }

        private void CancelChat() { CancellationTokenSource cancellation = chatCancellation; if (cancellation != null && !cancellation.IsCancellationRequested) cancellation.Cancel(); }

        private void UseChatAsFeature(object sender, EventArgs eventArgs)
        {
            ChatHistoryMessage latest = chatHistory.messages.LastOrDefault(delegate(ChatHistoryMessage message) { return message.role == "assistant"; });
            if (latest == null) return;
            specificationBox.Text = latest.text;
            leftTabs.SelectedTab = featureEditorTab;
            statusLabel.Text = "Draft updated";
            ownershipLabel.Text = "The assistant text was copied into the editor. It was not approved, queued, or run.";
        }

        private void RenderChatTranscript()
        {
            if (chatTranscriptBox == null) return;
            System.Text.StringBuilder text = new System.Text.StringBuilder();
            foreach (ChatHistoryMessage message in chatHistory.messages)
            {
                if (text.Length > 0) text.AppendLine().AppendLine();
                text.Append(message.role == "owner" ? "You" : (message.role == "assistant" ? RoleProvider.DisplayName(string.IsNullOrWhiteSpace(message.provider) ? RoleProvider.Local : message.provider) : "Error"));
                text.AppendLine(":");
                text.Append(message.text);
            }
            chatTranscriptBox.Text = text.ToString();
            chatTranscriptBox.SelectionStart = chatTranscriptBox.TextLength;
            chatTranscriptBox.ScrollToCaret();
            UpdateChatButtons();
        }

        private void PersistChatHistory()
        {
            try { ChatHistoryStore.Save(chatPath, chatHistory); }
            catch (Exception exception)
            {
                string message = "Chat history retention failed: " + exception.Message;
                statusLabel.Text = "Needs attention";
                ownershipLabel.Text = message;
                AppendLog("ERROR: " + message + Environment.NewLine);
            }
        }

        private async void OpenSettings(object sender, EventArgs eventArgs)
        {
            List<LocalModelOption> catalog = new List<LocalModelOption>();
            List<ChatGptModelOption> chatGptCatalog = new List<ChatGptModelOption>();
            try { catalog = await LocalModelCatalog.LoadAsync(settings.endpoint, CancellationToken.None); }
            catch (Exception exception) { AppendLog("Model catalog: " + exception.Message + Environment.NewLine); }
            try { chatGptCatalog = await ChatGptModelCatalog.LoadAsync(runnerPath, stateDirectory); }
            catch (Exception exception) { AppendLog("ChatGPT model catalog: " + exception.Message + Environment.NewLine); }
            GuiSettings draft = settings;
            while (true)
            {
                bool locked = IsGlobalConfigurationLocked();
                using (SettingsDialog dialog = new SettingsDialog(draft, locked, catalog, chatGptCatalog))
                {
                    DialogResult result = dialog.ShowDialog(this);
                    if (result == DialogResult.Cancel) return;
                    if (result == DialogResult.Retry)
                    {
                        try { GuiSettings requested = dialog.ReadSettings(settings, false); string endpointError = LoopbackEndpoint.Validate(requested.endpoint); if (endpointError != null) throw new InvalidDataException(endpointError); catalog = await LocalModelCatalog.LoadAsync(requested.endpoint, CancellationToken.None); chatGptCatalog = await ChatGptModelCatalog.LoadAsync(runnerPath, stateDirectory); draft = requested; statusLabel.Text = "Model catalogs reloaded"; }
                        catch (Exception exception) { draft = settings; SurfaceError("Could not reload settings: " + exception.Message); }
                        continue;
                    }
                    try
                    {
                        if (IsGlobalConfigurationLocked()) { SurfaceError("Settings became locked while the dialog was open. Wait for active work to finish, then reload."); continue; }
                        GuiSettings disk = GuiSettingsStore.Load(settingsPath);
                        if (disk.revision != settings.revision) { SurfaceError("Settings changed on disk. Reload before saving."); continue; }
                        GuiSettings updated = dialog.ReadSettings(settings, true);
                        GuiSettingsStore.Save(settingsPath, updated); settings = updated;
                        endpointBox.Text = settings.endpoint; modelBox.Text = settings.model; editorLocalReasoningEffort = settings.reasoning_effort;
                        editorOrchestratorProvider = settings.orchestrator_provider; editorOrchestratorModel = settings.orchestrator_model; editorOrchestratorReasoningEffort = settings.orchestrator_reasoning_effort;
                        editorReviewerProvider = settings.reviewer_provider; editorReviewerModel = settings.reviewer_model; editorReviewerReasoningEffort = settings.reviewer_reasoning_effort; attemptsBox.Value = Clamp(settings.default_attempts, 1, 20); timeoutBox.Value = Clamp(settings.timeout_seconds, 1, 3600);
                        statusLabel.Text = "Settings saved"; return;
                    }
                    catch (Exception exception) { SurfaceError("Could not save settings: " + exception.Message); }
                }
            }
        }

        private async void OpenSafety(object sender, EventArgs eventArgs)
        {
            while (true)
            {
                PermissionsDocument permissions;
                try { permissions = await NativeGuiControl.GetPermissionsAsync(runnerPath, stateDirectory); }
                catch (Exception exception) { SurfaceError("Could not load permissions: " + exception.Message); return; }
                using (PermissionsDialog dialog = new PermissionsDialog(permissions, IsGlobalConfigurationLocked()))
                {
                    DialogResult result = dialog.ShowDialog(this);
                    if (result == DialogResult.Cancel) return;
                    if (result == DialogResult.Retry) continue;
                    try { await NativeGuiControl.SetPermissionsAsync(runnerPath, stateDirectory, dialog.SelectedMode, permissions.revision); statusLabel.Text = "Permissions saved"; return; }
                    catch (Exception exception) { SurfaceError("Could not save permissions: " + exception.Message); }
                }
            }
        }

        private async void OpenGitHub(object sender, EventArgs eventArgs)
        {
            string project = repositoryBox == null ? string.Empty : repositoryBox.Text.Trim();
            using (GitHubDialog dialog = new GitHubDialog(project, IsGlobalConfigurationLocked()))
            {
                Func<Task> refreshAccount = async delegate
                {
                    dialog.SetBusy(true);
                    try { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "account", "--state-dir", stateDirectory); dialog.AccountStatus.Text = value.status == "signed_in" ? "Signed in as " + value.account : (value.message ?? "Signed out of GitHub"); }
                    catch (Exception exception) { dialog.Status.Text = exception.Message; }
                    finally { dialog.SetBusy(false); }
                };
                Func<Task> refreshConnection = async delegate
                {
                    if (string.IsNullOrWhiteSpace(dialog.ProjectPath.Text)) return;
                    try { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "connection", "get", "--state-dir", stateDirectory, "--project", Path.GetFullPath(dialog.ProjectPath.Text.Trim())); dialog.ApplyConnection(value); }
                    catch (Exception exception) { dialog.Status.Text = exception.Message; }
                };
                dialog.RefreshAccount.Click += async delegate { await refreshAccount(); };
                dialog.SignIn.Click += async delegate { await RunGitHubDialogOperation(dialog, async delegate { GitHubResponse value = await GitHubNative.RunStreamingAsync(githubPath, delegate(string progress) { dialog.SetProgress(progress.Trim()); }, "login", "--state-dir", stateDirectory); dialog.AccountStatus.Text = "Signed in as " + value.account; }); };
                dialog.CancelOperation.Click += async delegate { try { await GitHubNative.RunAsync(githubPath, "cancel", "--state-dir", stateDirectory); dialog.Status.Text = "GitHub operation cancellation requested"; } catch (Exception exception) { dialog.Status.Text = exception.Message; } };
                dialog.LoadRepositories.Click += async delegate { dialog.SetBusy(true); try { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "repositories", "--state-dir", stateDirectory); dialog.SetRepositories(value.repositories); dialog.Status.Text = "Repositories loaded"; } catch (Exception exception) { dialog.Status.Text = exception.Message; } finally { dialog.SetBusy(false); } };
                dialog.UseRepository.Click += delegate { GitHubRepository chosen = dialog.SelectedRepository; if (chosen == null) return; dialog.Repository.Text = chosen.nameWithOwner; dialog.BaseBranch.Text = chosen.defaultBranchRef == null ? string.Empty : chosen.defaultBranchRef.name; };
                dialog.CreateProject.Click += async delegate { await RunGitHubDialogOperation(dialog, async delegate { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "create-project", "--state-dir", stateDirectory, "--directory", Path.GetFullPath(dialog.ProjectPath.Text.Trim()), "--repository", dialog.Repository.Text.Trim()); dialog.Status.Text = value.message ?? "Empty local project ready. Save the connection separately."; }); };
                dialog.SaveConnection.Click += async delegate { await RunGitHubDialogOperation(dialog, async delegate { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "connection", "save", "--state-dir", stateDirectory, "--project", Path.GetFullPath(dialog.ProjectPath.Text.Trim()), "--repository", dialog.Repository.Text.Trim(), "--base-branch", dialog.BaseBranch.Text.Trim(), "--expected-revision", dialog.ConnectionRevision.ToString(System.Globalization.CultureInfo.InvariantCulture)); dialog.ApplyConnection(value); }); };
                dialog.Disconnect.Click += async delegate { await RunGitHubDialogOperation(dialog, async delegate { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "connection", "disconnect", "--state-dir", stateDirectory, "--project", Path.GetFullPath(dialog.ProjectPath.Text.Trim()), "--expected-revision", dialog.ConnectionRevision.ToString(System.Globalization.CultureInfo.InvariantCulture)); dialog.ConnectionRevision = value.revision; dialog.Status.Text = "Disconnected - successful features stay local and unpublished"; }); };
                dialog.CreateRepository.Click += async delegate
                {
                    string name = dialog.NewRepository.Text.Trim(), visibility = Convert.ToString(dialog.Visibility.SelectedItem);
                    if (MessageBox.Show(dialog, "Account: current GitHub account\r\nRepository: " + name + "\r\nVisibility: " + visibility + "\r\n\r\nCreate this repository with an initial README?", "Create GitHub repository?", MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
                    await RunGitHubDialogOperation(dialog, async delegate { GitHubResponse value = await GitHubNative.RunAsync(githubPath, "create-repository", "--state-dir", stateDirectory, "--name", name, "--visibility", visibility); if (value.repository != null) { dialog.Repository.Text = value.repository.nameWithOwner; dialog.BaseBranch.Text = value.repository.defaultBranchRef == null ? "main" : value.repository.defaultBranchRef.name; } dialog.Status.Text = value.status == "created" ? "Repository created. Create or select its empty local project, then save the connection." : "Repository already exists. Select it explicitly before saving."; });
                };
                await refreshAccount(); await refreshConnection(); dialog.ShowDialog(this);
            }
        }

        private async Task RunGitHubDialogOperation(GitHubDialog dialog, Func<Task> operation)
        {
            githubOperationActive = true; dialog.SetBusy(true); UpdateProviderLocks();
            try { await operation(); }
            catch (Exception exception) { dialog.Status.Text = exception.Message; }
            finally { githubOperationActive = false; dialog.SetBusy(false); UpdateProviderLocks(); }
        }

        private bool IsGlobalConfigurationLocked()
        {
            return activeProcess != null || githubOperationActive || chatCancellation != null || startGate.IsActive || queueLoopActive || (latestReport != null && latestReport.paused);
        }

        private void NewFeature(object sender, EventArgs eventArgs)
        {
            specificationBox.Clear(); modelBox.Text = settings.model; endpointBox.Text = settings.endpoint;
            editorLocalReasoningEffort = settings.reasoning_effort; editorOrchestratorProvider = settings.orchestrator_provider; editorOrchestratorModel = settings.orchestrator_model; editorOrchestratorReasoningEffort = settings.orchestrator_reasoning_effort;
            editorReviewerProvider = settings.reviewer_provider; editorReviewerModel = settings.reviewer_model; editorReviewerReasoningEffort = settings.reviewer_reasoning_effort;
            leftTabs.SelectedTab = featureEditorTab; specificationBox.Focus();
        }
        private AssemblyLineEntry SelectedLineEntry() { return assemblyLineList.SelectedItems.Count == 1 ? assemblyLineList.SelectedItems[0].Tag as AssemblyLineEntry : null; }

        private void RetrySelectedLine(object sender, EventArgs eventArgs)
        {
            AssemblyLineEntry selected = SelectedLineEntry();
            if (!AssemblyLineStore.Retry(selected)) return;
            PersistAssemblyLine();
            RefreshAssemblyLineList();
        }

        private void RemoveSelectedLine(object sender, EventArgs eventArgs)
        {
            AssemblyLineEntry selected = SelectedLineEntry();
            if (!AssemblyLineStore.Remove(assemblyLine, selected)) return;
            PersistAssemblyLine();
            RefreshAssemblyLineList();
        }

        private async Task ChangeSelectedReviewerAsync()
        {
            AssemblyLineEntry selected = SelectedLineEntry();
            if (selected == null || selected.status == "Running" || queueLoopActive) return;
            JobDocument job = JobJson.DeserializeJob(selected.frozen_job_json);
            List<ChatGptModelOption> chatGptCatalog = new List<ChatGptModelOption>();
            try { chatGptCatalog = await ChatGptModelCatalog.LoadAsync(runnerPath, stateDirectory); }
            catch (Exception exception) { AppendLog("ChatGPT model catalog: " + exception.Message + Environment.NewLine); }
            using (ReviewerDialog dialog = new ReviewerDialog(job, chatGptCatalog))
            {
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                dialog.Apply(job);
                string error = JobValidator.ValidateForForm(job, false); if (error != null) { SurfaceError(error); return; }
                selected.frozen_job_json = JobJson.Serialize(job); selected.result = "Reviewer changed by the owner. Start or retry explicitly when ready."; selected.updated_at_ms = AssemblyLineStore.NowMilliseconds(); PersistAssemblyLine(); RefreshAssemblyLineList();
            }
        }

        private async Task ReconcileSelectedPublicationAsync()
        {
            AssemblyLineEntry entry = SelectedLineEntry();
            if (entry == null || entry.publication_status != "attention" || string.IsNullOrWhiteSpace(entry.run_id) || IsGlobalConfigurationLocked()) return;
            string reportPath = Path.Combine(stateDirectory, "runs", entry.run_id, "report.json");
            if (!File.Exists(reportPath)) { SurfaceError("The retained run report required for publication reconciliation is missing."); return; }
            PipelineReport report;
            try { report = JobJson.DeserializeReport(SharedFile.ReadUtf8Json(reportPath)); }
            catch (Exception exception) { SurfaceError("Could not load the retained run report: " + exception.Message); return; }
            githubOperationActive = true; UpdateProviderLocks();
            try
            {
                GitHubResponse connection = await GitHubNative.RunAsync(githubPath, "connection", "get", "--state-dir", stateDirectory, "--project", Path.GetFullPath(entry.repository));
                GitHubResponse status = await GitHubNative.RunAsync(githubPath, "status", "--state-dir", stateDirectory);
                GitHubResponse result = await GitHubNative.RunAsync(githubPath, "reconcile", "--state-dir", stateDirectory, "--run-id", entry.run_id, "--expected-revision", status.revision.ToString(System.Globalization.CultureInfo.InvariantCulture));
                GitHubResponse durable = await GitHubNative.RunAsync(githubPath, "status", "--state-dir", stateDirectory); GitHubPublication persisted = null; bool retained = durable.publications != null && durable.publications.TryGetValue(entry.run_id, out persisted);
                if (result.status != "succeeded" || result.revision < status.revision || durable.revision != result.revision || !retained || !GitHubNative.SamePublication(result.publication, persisted) || !GitHubNative.ReconciledPublicationMatches(persisted, report, connection.connection) || !GitHubNative.DurableReceiptMatches(durable, entry.run_id, persisted)) throw new InvalidDataException("Durable GitHub reconciliation did not confirm this exact connection, run, receipt, and merged candidate.");
                entry.publication_status = "succeeded"; entry.publication_url = persisted.pull_request_url; entry.publication_merge_sha = persisted.merge_commit; entry.status = "Completed"; entry.stage = "complete"; entry.result = persisted.message ?? "Merged on GitHub after reconciliation."; entry.updated_at_ms = AssemblyLineStore.NowMilliseconds(); PersistAssemblyLine(); RefreshAssemblyLineList(); statusLabel.Text = "Publication reconciled";
            }
            catch (Exception exception) { SurfaceError("GitHub reconciliation needs attention: " + exception.Message); }
            finally { githubOperationActive = false; UpdateProviderLocks(); }
        }

        private void RefreshAssemblyLineList()
        {
            if (assemblyLineList == null) return;
            AssemblyLineEntry oldSelected = SelectedLineEntry();
            string selectedId = oldSelected == null ? null : oldSelected.id;
            assemblyLineList.BeginUpdate();
            try
            {
                assemblyLineList.Items.Clear();
                foreach (AssemblyLineEntry entry in assemblyLine.entries)
                {
                    ListViewItem item = new ListViewItem(entry.title ?? string.Empty);
                    item.SubItems.Add(entry.repository ?? string.Empty);
                    item.SubItems.Add(entry.status ?? string.Empty);
                    item.SubItems.Add(entry.stage ?? string.Empty);
                    item.SubItems.Add(entry.attempt.ToString());
                    item.SubItems.Add(entry.result ?? string.Empty);
                    item.Tag = entry;
                    if (entry.status == "Completed") item.ForeColor = Color.FromArgb(35, 116, 73);
                    else if (entry.status == "Failed" || entry.status == "Interrupted") item.ForeColor = Color.FromArgb(170, 43, 48);
                    assemblyLineList.Items.Add(item);
                    if (entry.id == selectedId) item.Selected = true;
                }
            }
            finally { assemblyLineList.EndUpdate(); }
            UpdateLineButtons();
        }

        private void PersistAssemblyLine()
        {
            if (!assemblyLineUsable) throw new InvalidOperationException("Assembly Line persistence is unavailable.");
            try { AssemblyLineStore.Save(linePath, assemblyLine); }
            catch (Exception exception)
            {
                assemblyLineUsable = false;
                queueStopRequested = true;
                SurfaceError("Assembly Line retention failed and the line was halted: " + exception.Message);
                throw;
            }
        }

        private void UpdateLineButtons()
        {
            if (startLineButton == null) return;
            AssemblyLineEntry selected = SelectedLineEntry();
            bool starting = startGate.IsActive;
            startLineButton.Enabled = assemblyLineUsable && !starting && !queueLoopActive && activeProcess == null && AssemblyLineStore.FirstPending(assemblyLine) != null;
            stopLineButton.Enabled = queueLoopActive || activeProcess != null;
            retryLineButton.Enabled = assemblyLineUsable && !queueLoopActive && selected != null && (selected.status == "Failed" || selected.status == "Cancelled" || selected.status == "Interrupted");
            removeLineButton.Enabled = assemblyLineUsable && selected != null && selected.status != "Running" && !queueLoopActive;
            changeReviewerButton.Enabled = assemblyLineUsable && selected != null && selected.status != "Running" && !queueLoopActive && activeProcess == null;
            reconcilePublicationButton.Enabled = assemblyLineUsable && selected != null && selected.publication_status == "attention" && !queueLoopActive && activeProcess == null && !githubOperationActive;
            newFeatureButton.Enabled = activeProcess == null;
        }

        private async void OnFormClosing(object sender, FormClosingEventArgs eventArgs)
        {
            closeRequested = true;
            CancelChat();
            queueStopRequested = true;
            SaveDraft();
            if (githubOperationActive)
            {
                eventArgs.Cancel = true;
                if (closingAfterCancel) return;
                if (!testMode && MessageBox.Show(this, "GitHub publication or setup is still active. Cancel it and close Assemblywright?", "Cancel GitHub operation", MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) { closeRequested = false; return; }
                closingAfterCancel = true;
                await CancelGitHubOperationAsync();
                closingAfterCancel = false; closeRequested = false;
                SurfaceError("Cancellation was requested. Assemblywright remains open until the GitHub operation reports a terminal result.");
                return;
            }
            if (allowClose || activeProcess == null) { statusTimer.Stop(); return; }
            eventArgs.Cancel = true;
            if (closingAfterCancel) return;
            if (!testMode)
            {
                DialogResult choice = MessageBox.Show(this, "A feature is still running. Cancel it and close Assemblywright?", "Cancel active feature", MessageBoxButtons.YesNo, MessageBoxIcon.Warning);
                if (choice != DialogResult.Yes) { closeRequested = false; return; }
            }
            closingAfterCancel = true;
            statusTimer.Stop();
            bool cancelAccepted = await InvokeControlAsync("cancel");
            if (!cancelAccepted) { closingAfterCancel = false; closeRequested = false; statusTimer.Start(); SurfaceError("Assemblywright remains open because the active job could not be cancelled."); return; }
            TaskCompletionSource<bool> processCompletion = activeProcessCompletion;
            if (processCompletion != null && !processCompletion.Task.IsCompleted)
            {
                Task winner = await Task.WhenAny(processCompletion.Task, Task.Delay(5000));
                if (winner != processCompletion.Task)
                {
                    closingAfterCancel = false; closeRequested = false; statusTimer.Start();
                    SurfaceError("Cancellation was accepted, but the active job has not stopped. Assemblywright remains open so its final state stays visible.");
                    return;
                }
            }
            if (processCompletion != null && (!processCompletion.Task.IsCompleted || !await processCompletion.Task))
            {
                closingAfterCancel = false; closeRequested = false; statusTimer.Start();
                SurfaceError("Cancellation was accepted, but the child process exit could not be confirmed. Assemblywright remains open for reconciliation.");
                return;
            }
            await RefreshStatusAsync(true);
            if (latestReport != null && latestReport.status == "running")
            {
                closingAfterCancel = false; closeRequested = false; statusTimer.Start();
                SurfaceError("The runner exited without a terminal report. Assemblywright remains open for reconciliation.");
                return;
            }
            allowClose = true;
            BeginInvoke(new MethodInvoker(delegate { Close(); }));
        }

        private JobDocument ReadJobFromForm()
        {
            JobDocument job = new JobDocument();
            job.repository = repositoryBox.Text.Trim();
            job.specification = specificationBox.Text;
            job.allowed_files = Lines(allowedFilesBox.Text, true);
            job.validation.program = validationProgramBox.Text.Trim();
            job.validation.args = Lines(validationArgumentsBox.Text, false);
            job.validation.timeout_seconds = Decimal.ToInt32(timeoutBox.Value);
            job.endpoint = endpointBox.Text.Trim();
            job.model = modelBox.Text.Trim();
            job.orchestrator_provider = editorOrchestratorProvider;
            job.orchestrator_model = editorOrchestratorModel;
            job.orchestrator_reasoning_effort = editorOrchestratorReasoningEffort;
            job.reviewer_provider = editorReviewerProvider;
            job.reviewer_model = string.IsNullOrWhiteSpace(editorReviewerModel) ? settings.reviewer_model : editorReviewerModel;
            job.reasoning_effort = string.IsNullOrWhiteSpace(editorLocalReasoningEffort) ? "none" : editorLocalReasoningEffort;
            job.reviewer_reasoning_effort = string.IsNullOrWhiteSpace(editorReviewerReasoningEffort) ? "none" : editorReviewerReasoningEffort;
            job.max_attempts = Decimal.ToInt32(attemptsBox.Value);
            return job;
        }

        private void PopulateForm(JobDocument job)
        {
            repositoryBox.Text = job.repository ?? string.Empty;
            specificationBox.Text = job.specification ?? string.Empty;
            allowedFilesBox.Text = string.Join(Environment.NewLine, job.allowed_files ?? new List<string>());
            validationProgramBox.Text = job.validation == null ? string.Empty : (job.validation.program ?? string.Empty);
            validationArgumentsBox.Text = job.validation == null ? string.Empty : string.Join(Environment.NewLine, job.validation.args ?? new List<string>());
            timeoutBox.Value = Clamp(job.validation == null || job.validation.timeout_seconds == 0 ? settings.timeout_seconds : job.validation.timeout_seconds, 1, 3600);
            endpointBox.Text = string.IsNullOrEmpty(job.endpoint) ? settings.endpoint : job.endpoint;
            modelBox.Text = string.IsNullOrEmpty(job.model) ? settings.model : job.model;
            editorLocalReasoningEffort = string.IsNullOrWhiteSpace(job.reasoning_effort) ? settings.reasoning_effort : job.reasoning_effort;
            editorOrchestratorProvider = job.orchestrator_provider; editorOrchestratorModel = job.orchestrator_model; editorOrchestratorReasoningEffort = job.orchestrator_reasoning_effort;
            editorReviewerProvider = job.reviewer_provider; editorReviewerModel = RoleSettings.ReviewerModelForJob(job, settings);
            editorReviewerReasoningEffort = string.IsNullOrWhiteSpace(job.reviewer_reasoning_effort) ? settings.reviewer_reasoning_effort : job.reviewer_reasoning_effort;
            attemptsBox.Value = Clamp(job.max_attempts == 0 ? settings.default_attempts : job.max_attempts, 1, 20);
        }

        private void LoadJob(string path, bool requireValid)
        {
            JobDocument job = JobJson.DeserializeJob(SharedFile.ReadUtf8Json(path));
            if (requireValid)
            {
                string validation = JobValidator.ValidateForForm(job, false);
                if (validation != null) throw new InvalidDataException(validation);
            }
            PopulateForm(job);
            statusLabel.Text = "Job loaded";
            AppendLog("Loaded job: " + Path.GetFullPath(path) + Environment.NewLine);
        }

        private void SaveDraft()
        {
            try { JobJson.WriteUtf8(draftPath, JobJson.Serialize(ReadJobFromForm())); }
            catch (Exception exception) { AppendLog("ERROR: Could not retain the job draft: " + exception.Message + Environment.NewLine); }
        }

        private void LoadJobClicked(object sender, EventArgs eventArgs)
        {
            if (testMode) { SurfaceError("File dialogs are disabled in test mode. Use --job."); return; }
            using (OpenFileDialog dialog = new OpenFileDialog())
            {
                dialog.Filter = "Assemblywright job (*.json)|*.json|All files (*.*)|*.*";
                dialog.CheckFileExists = true;
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                try { LoadJob(dialog.FileName, true); }
                catch (Exception exception) { SurfaceError("Could not load the job: " + exception.Message); }
            }
        }

        private void SaveJobClicked(object sender, EventArgs eventArgs)
        {
            JobDocument job = ReadJobFromForm();
            string validation = JobValidator.ValidateForForm(job, false);
            if (validation != null) { SurfaceError(validation); return; }
            if (testMode) { SurfaceError("File dialogs are disabled in test mode."); return; }
            using (SaveFileDialog dialog = new SaveFileDialog())
            {
                dialog.Filter = "Assemblywright job (*.json)|*.json";
                dialog.DefaultExt = "json";
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                try { JobJson.WriteUtf8(dialog.FileName, JobJson.Serialize(job)); statusLabel.Text = "Job saved"; }
                catch (Exception exception) { SurfaceError("Could not save the job: " + exception.Message); }
            }
        }

        private void BrowseRepository(object sender, EventArgs eventArgs)
        {
            if (testMode) { SurfaceError("Folder dialogs are disabled in test mode."); return; }
            using (FolderBrowserDialog dialog = new FolderBrowserDialog())
            {
                dialog.Description = "Choose the clean Git repository";
                dialog.SelectedPath = repositoryBox.Text;
                if (dialog.ShowDialog(this) == DialogResult.OK) repositoryBox.Text = dialog.SelectedPath;
            }
        }

        private void BrowseValidationProgram(object sender, EventArgs eventArgs)
        {
            if (testMode) { SurfaceError("File dialogs are disabled in test mode."); return; }
            using (OpenFileDialog dialog = new OpenFileDialog())
            {
                dialog.Filter = "Programs (*.exe;*.cmd;*.bat)|*.exe;*.cmd;*.bat|All files (*.*)|*.*";
                dialog.CheckFileExists = true;
                if (dialog.ShowDialog(this) == DialogResult.OK) validationProgramBox.Text = dialog.FileName;
            }
        }

        private void OpenCandidate(object sender, EventArgs eventArgs)
        {
            string candidate = latestReport == null ? string.Empty : UsableWindowsPath(latestReport.candidate);
            if (latestReport == null || latestReport.status != "completed" || !Directory.Exists(candidate)) return;
            try { Process.Start(new ProcessStartInfo("explorer.exe", WindowsCommandLine.Quote(candidate)) { UseShellExecute = false }); }
            catch (Exception exception) { SurfaceError("Could not open the candidate: " + exception.Message); }
        }

        private void OpenReport(object sender, EventArgs eventArgs)
        {
            if (latestReport == null) return;
            string path = ReportPath(latestReport);
            if (!File.Exists(path)) return;
            try { Process.Start(new ProcessStartInfo(path) { UseShellExecute = true }); }
            catch (Exception exception) { SurfaceError("Could not open the report: " + exception.Message); }
        }

        private string ReportPath(PipelineReport report) { return string.IsNullOrEmpty(report.run_id) ? string.Empty : Path.Combine(stateDirectory, "runs", report.run_id, "report.json"); }

        private void AppendLog(string value)
        {
            if (IsDisposed) return;
            if (InvokeRequired)
            {
                try { BeginInvoke(new Action<string>(AppendLog), value); }
                catch (InvalidOperationException) { }
                return;
            }
            if (logBox == null) return;
            logBox.Text = boundedLog.Append(value);
            logBox.SelectionStart = logBox.TextLength;
            logBox.ScrollToCaret();
        }

        private void SurfaceError(string message)
        {
            if (statusLabel != null) statusLabel.Text = "Needs attention";
            if (stageLabel != null) stageLabel.Text = "Stage: error";
            if (ownershipLabel != null) ownershipLabel.Text = message;
            AppendLog("ERROR: " + message + Environment.NewLine);
        }

        internal void SurfaceUnhandled(Exception exception) { SurfaceError("Unexpected application error: " + exception.Message); }

        private void ScheduleScreenshot()
        {
            if (string.IsNullOrEmpty(screenshotPath)) return;
            System.Windows.Forms.Timer screenshotTimer = new System.Windows.Forms.Timer();
            screenshotTimer.Interval = 500;
            screenshotTimer.Tick += delegate
            {
                screenshotTimer.Stop();
                try
                {
                    string parent = Path.GetDirectoryName(screenshotPath);
                    if (!string.IsNullOrEmpty(parent)) Directory.CreateDirectory(parent);
                    using (Bitmap bitmap = new Bitmap(Width, Height))
                    {
                        DrawToBitmap(bitmap, new Rectangle(0, 0, Width, Height));
                        bitmap.Save(screenshotPath, System.Drawing.Imaging.ImageFormat.Png);
                    }
                }
                catch (Exception exception) { SurfaceError("Could not save the screenshot: " + exception.Message); }
                finally { screenshotTimer.Dispose(); }
            };
            screenshotTimer.Start();
        }

        private void SetInputsEnabled(bool enabled)
        {
            repositoryBox.Enabled = enabled; repositoryBrowseButton.Enabled = enabled; specificationBox.Enabled = enabled;
            allowedFilesBox.Enabled = enabled; validationProgramBox.Enabled = enabled; validationBrowseButton.Enabled = enabled;
            validationArgumentsBox.Enabled = enabled; timeoutBox.Enabled = enabled; attemptsBox.Enabled = enabled;
            endpointBox.Enabled = enabled; modelBox.Enabled = enabled; loadJobButton.Enabled = enabled; saveJobButton.Enabled = enabled;
            runFeatureButton.Enabled = enabled; runDemoButton.Enabled = enabled; addToLineButton.Enabled = enabled;
            UpdateStartActionButtons();
        }

        private bool TryBeginStartOperation()
        {
            if (!startGate.TryEnter()) return false;
            UpdateStartActionButtons();
            UpdateProviderLocks();
            return true;
        }

        private void EndStartOperation()
        {
            startGate.Exit();
            if (IsDisposed) return;
            UpdateStartActionButtons();
            UpdateProviderLocks();
        }

        private void UpdateStartActionButtons()
        {
            if (runFeatureButton == null) return;
            bool starting = startGate.IsActive;
            if (starting)
            {
                runFeatureButton.Enabled = false;
                runDemoButton.Enabled = false;
                addToLineButton.Enabled = false;
            }
            else
            {
                bool paused = latestReport != null && latestReport.paused;
                bool available = activeProcess == null && !queueLoopActive;
                runFeatureButton.Enabled = available && !paused;
                runDemoButton.Enabled = available && !paused;
                addToLineButton.Enabled = available;
            }
            UpdateLineButtons();
        }

        private void UpdateProviderLocks()
        {
            bool locked = activeProcess != null || githubOperationActive || chatCancellation != null || startGate.IsActive;
            if (endpointBox != null) endpointBox.Enabled = !locked;
            if (modelBox != null) modelBox.Enabled = !locked;
        }

        private void UpdateControlButtons(PipelineReport report)
        {
            bool running = report != null && report.status == "running";
            bool paused = report != null && report.paused;
            pauseButton.Enabled = !paused;
            cancelButton.Enabled = running;
            resumeButton.Enabled = paused;
            UpdateChatButtons();
            UpdateStartActionButtons();
        }

        private void UpdateChatButtons()
        {
            if (chatSendButton == null) return;
            bool active = chatCancellation != null;
            bool paused = latestReport != null && latestReport.paused;
            chatSendButton.Enabled = !active && !paused;
            chatStopButton.Enabled = active;
            chatUseFeatureButton.Enabled = chatHistory.messages.Any(delegate(ChatHistoryMessage message) { return message.role == "assistant"; });
        }

        private void SetControlButtonsEnabled(bool enabled)
        {
            pauseButton.Enabled = enabled && (latestReport == null || !latestReport.paused);
            cancelButton.Enabled = enabled && latestReport != null && latestReport.status == "running";
            resumeButton.Enabled = enabled && latestReport != null && latestReport.paused;
        }

        private static string DisplayStatus(string status, bool paused) { if (paused && status != "running") return "Paused"; if (string.IsNullOrEmpty(status)) return "Unknown"; return char.ToUpperInvariant(status[0]) + status.Substring(1); }
        private static string FirstUsefulLine(string text) { if (string.IsNullOrWhiteSpace(text)) return string.Empty; return text.Trim().Split(new char[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries)[0]; }
        private static string UsableWindowsPath(string path) { if (string.IsNullOrEmpty(path)) return string.Empty; if (path.StartsWith("\\\\?\\UNC\\", StringComparison.OrdinalIgnoreCase)) return "\\\\" + path.Substring(8); if (path.StartsWith("\\\\?\\", StringComparison.OrdinalIgnoreCase)) return path.Substring(4); return path; }

        private static List<string> Lines(string text, bool trim)
        {
            List<string> values = new List<string>();
            foreach (string line in (text ?? string.Empty).Replace("\r\n", "\n").Split('\n'))
            {
                string value = trim ? line.Trim() : line.TrimEnd('\r');
                if (value.Length > 0) values.Add(value);
            }
            return values;
        }

        private static decimal Clamp(int value, int minimum, int maximum) { return Math.Max(minimum, Math.Min(maximum, value)); }
        private static NumericUpDown CreateNumber(string name, int minimum, int maximum, int value) { NumericUpDown n = new NumericUpDown(); n.Name = name; n.AccessibleName = name; n.Minimum = minimum; n.Maximum = maximum; n.Value = Math.Max(minimum, Math.Min(maximum, value)); n.Dock = DockStyle.Fill; return n; }
        private static Panel CreateCard() { Panel p = new Panel(); p.BackColor = Color.White; p.BorderStyle = BorderStyle.FixedSingle; return p; }
        private static TextBox CreateTextBox(string name, bool multiline) { TextBox b = new TextBox(); b.Name = name; b.AccessibleName = name; b.Multiline = multiline; b.ScrollBars = multiline ? ScrollBars.Vertical : ScrollBars.None; b.Dock = DockStyle.Fill; b.BorderStyle = BorderStyle.FixedSingle; return b; }

        private static Button CreateButton(string text, string name, EventHandler action)
        {
            Button b = new Button(); b.Text = text; b.Name = name; b.AccessibleName = name; b.AutoSize = true; b.MinimumSize = new Size(94, 32);
            b.FlatStyle = FlatStyle.Flat; b.FlatAppearance.BorderColor = Color.FromArgb(177, 187, 201); b.BackColor = Color.White; b.Click += action; return b;
        }

        private static Button CreatePrimaryButton(string text, string name, EventHandler action) { Button b = CreateButton(text, name, action); b.BackColor = Color.FromArgb(38, 103, 210); b.ForeColor = Color.White; b.FlatAppearance.BorderColor = b.BackColor; b.Font = new Font("Segoe UI Semibold", 9F, FontStyle.Bold); return b; }
        private static Button CreateDangerButton(string text, string name, EventHandler action) { Button b = CreateButton(text, name, action); b.ForeColor = Color.FromArgb(170, 43, 48); b.FlatAppearance.BorderColor = Color.FromArgb(214, 156, 159); return b; }
        private static Label CreateFieldLabel(string text) { Label l = new Label(); l.Text = text; l.Dock = DockStyle.Fill; l.TextAlign = ContentAlignment.MiddleLeft; l.ForeColor = Color.FromArgb(52, 62, 78); return l; }
        private static Label CreateStatusLabel(string name, string text) { Label l = CreateFieldLabel(text); l.Name = name; l.AutoEllipsis = true; return l; }
        private static void AddSectionTitle(TableLayoutPanel table, string text) { Label l = new Label(); l.Text = text; l.Dock = DockStyle.Fill; l.Font = new Font("Segoe UI Semibold", 12F, FontStyle.Bold); l.ForeColor = Color.FromArgb(27, 37, 54); AddRow(table, l, 35); }

        private static void AddLabeledControl(TableLayoutPanel table, string label, Control control, int height)
        {
            TableLayoutPanel h = new TableLayoutPanel(); h.Dock = DockStyle.Fill; h.ColumnCount = 1; h.RowCount = 2;
            h.RowStyles.Add(new RowStyle(SizeType.Absolute, 24)); h.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            h.Controls.Add(CreateFieldLabel(label), 0, 0); h.Controls.Add(control, 0, 1); AddRow(table, h, height);
        }

        private static void AddLabeledPicker(TableLayoutPanel table, string label, Control control, Button button)
        {
            TableLayoutPanel h = new TableLayoutPanel(); h.Dock = DockStyle.Fill; h.ColumnCount = 2; h.RowCount = 2;
            h.RowStyles.Add(new RowStyle(SizeType.Absolute, 24)); h.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
            h.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); h.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 100));
            Label l = CreateFieldLabel(label); h.Controls.Add(l, 0, 0); h.SetColumnSpan(l, 2); h.Controls.Add(control, 0, 1); h.Controls.Add(button, 1, 1); AddRow(table, h, 62);
        }

        private static void AddRow(TableLayoutPanel table, Control control, int height) { AddRow(table, control, height, SizeType.Absolute); }
        private static void AddRow(TableLayoutPanel table, Control control, int height, SizeType sizeType) { int row = table.RowStyles.Count; table.RowStyles.Add(new RowStyle(sizeType, height)); table.Controls.Add(control, 0, row); }

        private static class NativeMethods
        {
            [DllImport("user32.dll")]
            [return: MarshalAs(UnmanagedType.Bool)]
            internal static extern bool ShowWindow(IntPtr window, int command);
        }
    }
}
