using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace Assemblywright.Windows
{
    internal sealed class GitHubRepository { public string nameWithOwner { get; set; } public string url { get; set; } public string visibility { get; set; } public GitHubBranch defaultBranchRef { get; set; } public GitHubRepository() { } }
    internal sealed class GitHubBranch { public string name { get; set; } public GitHubBranch() { } }
    internal sealed class GitHubConnection { public long revision { get; set; } public string project { get; set; } public string repository { get; set; } public string repository_url { get; set; } public string base_branch { get; set; } public string source_head { get; set; } public object saved_at_ms { get; set; } public GitHubConnection() { } }
    internal sealed class GitHubPublication
    {
        public string run_id { get; set; } public string project { get; set; } public string repository { get; set; } public string base_branch { get; set; }
        public long connection_revision { get; set; } public long completion_revision { get; set; }
        public string source_head { get; set; } public string candidate_commit { get; set; } public string branch { get; set; } public string status { get; set; }
        public string stage { get; set; } public object pull_request_number { get; set; } public string pull_request_url { get; set; } public string merge_commit { get; set; } public string message { get; set; }
        public GitHubPublication() { }
    }
    internal sealed class GitHubPublicationReceipt { public bool verified { get; set; } public long revision { get; set; } public GitHubPublicationReceipt() { } }
    internal sealed class GitHubResponse
    {
        public int schema_version { get; set; } public string status { get; set; } public string message { get; set; } public string account { get; set; }
        public long revision { get; set; } public List<GitHubRepository> repositories { get; set; } public GitHubRepository repository { get; set; }
        public GitHubConnection connection { get; set; } public string project { get; set; } public bool empty { get; set; } public bool prepared_checkout { get; set; }
        public GitHubPublication publication { get; set; }
        public Dictionary<string, GitHubPublication> publications { get; set; }
        public Dictionary<string, GitHubPublicationReceipt> publication_receipts { get; set; }
        public GitHubResponse() { repositories = new List<GitHubRepository>(); publications = new Dictionary<string, GitHubPublication>(); publication_receipts = new Dictionary<string, GitHubPublicationReceipt>(); }
    }

    internal static class GitHubNative
    {
        internal static bool PublicationMatches(GitHubPublication publication, PipelineReport report, GitHubConnection connection)
        {
            return PublicationMatchesCommon(publication, report, connection)
                && publication.connection_revision == connection.revision
                && !string.IsNullOrWhiteSpace(connection.source_head)
                && string.Equals(publication.source_head, connection.source_head, StringComparison.Ordinal);
        }

        internal static bool ReconciledPublicationMatches(GitHubPublication publication, PipelineReport report, GitHubConnection currentConnection)
        {
            if (!PublicationMatchesCommon(publication, report, currentConnection) || string.IsNullOrWhiteSpace(currentConnection.source_head)) return false;
            bool originalConnection = publication.connection_revision == currentConnection.revision && string.Equals(publication.source_head, currentConnection.source_head, StringComparison.Ordinal);
            bool confirmedFastForward = currentConnection.revision > publication.connection_revision && string.Equals(currentConnection.source_head, publication.merge_commit, StringComparison.Ordinal);
            return originalConnection || confirmedFastForward;
        }

        private static bool PublicationMatchesCommon(GitHubPublication publication, PipelineReport report, GitHubConnection connection)
        {
            if (publication == null || report == null || connection == null || string.IsNullOrWhiteSpace(publication.project) || string.IsNullOrWhiteSpace(connection.project)) return false;
            string publicationProject, connectionProject;
            if (!TryNormalizeProjectPath(publication.project, out publicationProject) || !TryNormalizeProjectPath(connection.project, out connectionProject)) return false;
            return publication.status == "succeeded"
                && string.Equals(publication.run_id, report.run_id, StringComparison.Ordinal)
                && string.Equals(publication.candidate_commit, report.candidate_commit, StringComparison.Ordinal)
                && string.Equals(publicationProject, connectionProject, StringComparison.OrdinalIgnoreCase)
                && string.Equals(publication.repository, connection.repository, StringComparison.OrdinalIgnoreCase)
                && string.Equals(publication.base_branch, connection.base_branch, StringComparison.Ordinal)
                && publication.completion_revision > publication.connection_revision
                && !string.IsNullOrWhiteSpace(publication.source_head)
                && string.Equals(publication.branch, "assemblywright/" + report.run_id, StringComparison.Ordinal)
                && ValidPullRequest(publication.pull_request_url, connection.repository)
                && PullRequestNumberMatches(publication.pull_request_url, publication.pull_request_number)
                && ValidHexCommit(publication.merge_commit);
        }

        internal static bool TryNormalizeProjectPath(string value, out string normalized)
        {
            normalized = null;
            if (string.IsNullOrWhiteSpace(value) || !string.Equals(value, value.Trim(), StringComparison.Ordinal)) return false;
            string candidate = value;
            if (candidate.StartsWith(@"\\?\", StringComparison.OrdinalIgnoreCase))
            {
                string verbatim = candidate.Substring(4);
                if (verbatim.StartsWith(@"UNC\", StringComparison.OrdinalIgnoreCase))
                {
                    string unc = verbatim.Substring(4);
                    if (!IsAbsoluteUncPath(@"\\" + unc)) return false;
                    candidate = @"\\" + unc;
                }
                else
                {
                    if (!IsAbsoluteDrivePath(verbatim)) return false;
                    candidate = verbatim;
                }
            }
            else if (candidate.StartsWith(@"\\.\", StringComparison.OrdinalIgnoreCase) || candidate.StartsWith(@"\??\", StringComparison.OrdinalIgnoreCase)) return false;

            if (!IsCanonicalProjectPath(candidate)) return false;
            try { normalized = Path.GetFullPath(candidate); }
            catch (ArgumentException) { return false; }
            catch (NotSupportedException) { return false; }
            catch (PathTooLongException) { return false; }
            catch (System.Security.SecurityException) { return false; }
            return (IsAbsoluteDrivePath(normalized) || IsAbsoluteUncPath(normalized))
                && string.Equals(candidate, normalized, StringComparison.OrdinalIgnoreCase);
        }

        private static bool IsCanonicalProjectPath(string value)
        {
            if (value.IndexOf('/') >= 0) return false;
            string components;
            if (IsAbsoluteDrivePath(value))
            {
                if (value.IndexOf(':', 2) >= 0) return false;
                components = value.Substring(3);
                if (components.Length == 0) return true;
            }
            else if (IsAbsoluteUncPath(value))
            {
                if (value.IndexOf(':') >= 0) return false;
                components = value.Substring(2);
            }
            else return false;

            string[] parts = components.Split(new char[] { '\\' });
            if (parts.Length == 0) return false;
            foreach (string part in parts)
            {
                if (part.Length == 0 || part == "." || part == ".." || part.EndsWith(".", StringComparison.Ordinal) || part.EndsWith(" ", StringComparison.Ordinal)) return false;
                if (part.IndexOfAny(new char[] { '<', '>', '"', '|', '?', '*' }) >= 0) return false;
                foreach (char character in part) if (character < 32) return false;
                string device = part.Split(new char[] { '.' }, 2)[0];
                if (device.Equals("CON", StringComparison.OrdinalIgnoreCase) || device.Equals("PRN", StringComparison.OrdinalIgnoreCase)
                    || device.Equals("AUX", StringComparison.OrdinalIgnoreCase) || device.Equals("NUL", StringComparison.OrdinalIgnoreCase)
                    || device.Equals("CLOCK$", StringComparison.OrdinalIgnoreCase)
                    || (device.Length == 4 && (device.StartsWith("COM", StringComparison.OrdinalIgnoreCase) || device.StartsWith("LPT", StringComparison.OrdinalIgnoreCase)) && device[3] >= '1' && device[3] <= '9')) return false;
            }
            return true;
        }

        private static bool IsAbsoluteDrivePath(string value)
        {
            return value != null && value.Length >= 3 && ((value[0] >= 'A' && value[0] <= 'Z') || (value[0] >= 'a' && value[0] <= 'z'))
                && value[1] == ':' && (value[2] == '\\' || value[2] == '/');
        }

        private static bool IsAbsoluteUncPath(string value)
        {
            if (value == null || value.Length < 5 || value[0] != '\\' || value[1] != '\\') return false;
            int serverEnd = value.IndexOfAny(new char[] { '\\', '/' }, 2);
            if (serverEnd <= 2) return false;
            int shareStart = serverEnd + 1;
            if (shareStart >= value.Length) return false;
            int shareEnd = value.IndexOfAny(new char[] { '\\', '/' }, shareStart);
            string server = value.Substring(2, serverEnd - 2);
            string share = shareEnd < 0 ? value.Substring(shareStart) : value.Substring(shareStart, shareEnd - shareStart);
            return server.Length > 0 && share.Length > 0 && server != "." && server != ".." && share != "." && share != "..";
        }

        internal static bool SamePublication(GitHubPublication left, GitHubPublication right)
        {
            return left != null && right != null && left.run_id == right.run_id && left.project == right.project && left.repository == right.repository
                && left.base_branch == right.base_branch && left.source_head == right.source_head && left.candidate_commit == right.candidate_commit
                && left.connection_revision == right.connection_revision && left.completion_revision == right.completion_revision
                && left.branch == right.branch && left.status == right.status && left.stage == right.stage
                && Convert.ToString(left.pull_request_number) == Convert.ToString(right.pull_request_number)
                && left.pull_request_url == right.pull_request_url && left.merge_commit == right.merge_commit;
        }

        internal static bool DurableReceiptMatches(GitHubResponse durable, string runId, GitHubPublication publication)
        {
            GitHubPublicationReceipt receipt;
            return durable != null && durable.status == "ok" && publication != null && publication.completion_revision > 0
                && durable.revision >= publication.completion_revision && durable.publication_receipts != null
                && durable.publication_receipts.TryGetValue(runId, out receipt) && receipt != null && receipt.verified
                && receipt.revision == publication.completion_revision;
        }

        private static bool ValidBranch(string branch, string baseBranch)
        {
            return !string.IsNullOrWhiteSpace(branch) && branch != baseBranch && !branch.StartsWith("/", StringComparison.Ordinal) && !branch.EndsWith("/", StringComparison.Ordinal)
                && branch.IndexOf("..", StringComparison.Ordinal) < 0 && branch.IndexOf("@{", StringComparison.Ordinal) < 0 && branch.IndexOfAny(new char[] { '\\', '~', '^', ':', '?', '*', '[', ' ', '\r', '\n' }) < 0;
        }

        private static bool PullRequestNumberMatches(string url, object reported)
        {
            Uri parsed; long value; if (!Uri.TryCreate(url, UriKind.Absolute, out parsed)) return false; string[] parts = parsed.AbsolutePath.Trim('/').Split(new char[] { '/' }); string last = parts.Length == 0 ? string.Empty : parts[parts.Length - 1]; return long.TryParse(last, out value) && value > 0 && Convert.ToString(reported, System.Globalization.CultureInfo.InvariantCulture) == value.ToString(System.Globalization.CultureInfo.InvariantCulture);
        }

        private static bool ValidPullRequest(string value, string repository)
        {
            Uri url; if (!Uri.TryCreate(value, UriKind.Absolute, out url) || url.Scheme != Uri.UriSchemeHttps || !url.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase) || !string.IsNullOrEmpty(url.UserInfo) || !url.IsDefaultPort || !string.IsNullOrEmpty(url.Query) || !string.IsNullOrEmpty(url.Fragment)) return false;
            string[] expected = (repository ?? string.Empty).Split('/'), parts = url.AbsolutePath.Trim('/').Split('/'); int number;
            return expected.Length == 2 && parts.Length == 4 && parts[0].Equals(expected[0], StringComparison.OrdinalIgnoreCase) && parts[1].Equals(expected[1], StringComparison.OrdinalIgnoreCase) && parts[2] == "pull" && int.TryParse(parts[3], out number) && number > 0;
        }

        private static bool ValidHexCommit(string value)
        {
            if (value == null || (value.Length != 40 && value.Length != 64)) return false;
            foreach (char character in value) if ((character < '0' || character > '9') && (character < 'a' || character > 'f')) return false;
            return true;
        }

        internal static async Task<GitHubResponse> RunAsync(string executable, params string[] arguments)
        {
            if (!File.Exists(executable)) throw new FileNotFoundException("The packaged GitHub adapter is missing.", executable);
            ProcessResult result = await ProcessRunner.RunCapturedAsync(executable, arguments);
            GitHubResponse response = null;
            try { response = Serializer.New().Deserialize<GitHubResponse>(result.StandardOutput); } catch (Exception) { }
            if (result.ExitCode != 0 || response == null || response.schema_version != 1 || response.status == "error")
            {
                string message = response == null ? null : response.message;
                if (string.IsNullOrWhiteSpace(message)) message = string.IsNullOrWhiteSpace(result.StandardError) ? "The GitHub operation failed." : result.StandardError.Trim();
                throw new InvalidOperationException(message);
            }
            return response;
        }

        internal static async Task<GitHubResponse> RunStreamingAsync(string executable, Action<string> progress, params string[] arguments)
        {
            if (!File.Exists(executable)) throw new FileNotFoundException("The packaged GitHub adapter is missing.", executable);
            BoundedCapture output = new BoundedCapture(2 * 1024 * 1024), error = new BoundedCapture(2 * 1024 * 1024);
            Process process = ProcessRunner.StartStreaming(executable, arguments, output.Append, delegate(string value) { error.Append(value); if (progress != null) progress(value); });
            await Task.Run(delegate { process.WaitForExit(); }); int exitCode = process.ExitCode; process.Dispose();
            if (output.Overflowed || error.Overflowed) throw new InvalidDataException("The GitHub adapter output exceeded 2 MiB.");
            GitHubResponse response = null; try { response = Serializer.New().Deserialize<GitHubResponse>(output.Text); } catch (Exception) { }
            if (exitCode != 0 || response == null || response.schema_version != 1 || response.status == "error")
            {
                string message = response == null ? null : response.message; if (string.IsNullOrWhiteSpace(message)) message = string.IsNullOrWhiteSpace(error.Text) ? "The GitHub operation failed." : error.Text.Trim(); throw new InvalidOperationException(message);
            }
            return response;
        }
    }

    internal sealed class GitHubDialog : Form
    {
        internal readonly Label AccountStatus, Status;
        internal readonly Button RefreshAccount, SignIn, CancelOperation, LoadRepositories, UseRepository, CreateRepository, CreateProject, SaveConnection, Disconnect, Done;
        internal readonly ComboBox RepositoryList, Visibility;
        internal readonly TextBox Repository, BaseBranch, ProjectPath, NewRepository;
        internal long ConnectionRevision { get; set; }
        private bool busy;
        private readonly bool configurationLocked;

        internal GitHubDialog(string projectPath, bool locked)
        {
            Name = "GitHubDialog"; AccessibleName = "GitHub publication"; Text = "GitHub publication";
            configurationLocked = locked;
            StartPosition = FormStartPosition.CenterParent; MinimumSize = new Size(700, 720); ClientSize = new Size(720, 760); Font = new Font("Segoe UI", 9F);
            TableLayoutPanel root = new TableLayoutPanel(); root.Dock = DockStyle.Fill; root.Padding = new Padding(20); root.ColumnCount = 1; root.AutoScroll = true; Controls.Add(root);
            Label title = Label("GitHub publication", "GitHubTitle"); title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold); Add(root, title, 38);
            Add(root, Label("Connect an existing repository. Future successful features use their own branch, open a pull request, and merge automatically after required checks pass.", "GitHubHelpLabel"), 46);

            AccountStatus = Label("GitHub account not loaded", "GitHubAccountStatusLabel"); AccountStatus.AccessibleName = null; Add(root, AccountStatus, 30);
            FlowLayoutPanel accountActions = Actions(); RefreshAccount = Button("Refresh account", "GitHubRefreshButton"); SignIn = Button("Sign in to GitHub", "GitHubSignInButton"); CancelOperation = Button("Cancel operation", "GitHubCancelButton");
            accountActions.Controls.Add(RefreshAccount); accountActions.Controls.Add(SignIn); accountActions.Controls.Add(CancelOperation); Add(root, accountActions, 42);

            LoadRepositories = Button("Load repositories", "GitHubLoadRepositoriesButton"); RepositoryList = Combo("GitHubRepositoryList"); UseRepository = Button("Use project", "GitHubUseRepositoryButton");
            TableLayoutPanel list = new TableLayoutPanel(); list.Dock = DockStyle.Fill; list.ColumnCount = 3; list.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 135)); list.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); list.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 110)); list.Controls.Add(LoadRepositories, 0, 0); list.Controls.Add(RepositoryList, 1, 0); list.Controls.Add(UseRepository, 2, 0); Add(root, list, 43);

            ProjectPath = TextBoxFor(projectPath, "GitHubProjectPathBox"); AddField(root, "Local project", "Create one empty folder under the Windows workspace. This does not clone, download, upload, or connect the repository.", ProjectPath, 72);
            CreateProject = Button("Create empty local project", "GitHubCreateLocalButton"); Add(root, CreateProject, 39);
            Repository = TextBoxFor(string.Empty, "GitHubRepositoryBox"); AddField(root, "Repository", "owner/repository", Repository, 59);
            BaseBranch = TextBoxFor("main", "GitHubBaseBranchBox"); AddField(root, "Base branch", "Save connection downloads and verifies this baseline in an empty project. Automatic merge occurs only after required checks pass.", BaseBranch, 68);
            FlowLayoutPanel connectionActions = Actions(); SaveConnection = Button("Save connection", "GitHubSaveButton"); Disconnect = Button("Disconnect", "GitHubDisconnectButton"); connectionActions.Controls.Add(SaveConnection); connectionActions.Controls.Add(Disconnect); Add(root, connectionActions, 42);

            NewRepository = TextBoxFor(string.Empty, "GitHubCreateNameBox"); AddField(root, "Create a repository", "Creates a repository initialized with a README. Review the owner and visibility before creation.", NewRepository, 72);
            Visibility = Combo("GitHubVisibilityBox"); Visibility.DropDownStyle = ComboBoxStyle.DropDownList; Visibility.Items.AddRange(new object[] { "private", "public" }); Visibility.SelectedIndex = 0;
            CreateRepository = Button("Review repository creation...", "GitHubCreateRepoButton"); TableLayoutPanel creation = new TableLayoutPanel(); creation.Dock = DockStyle.Fill; creation.ColumnCount = 2; creation.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); creation.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 210)); creation.Controls.Add(Visibility, 0, 0); creation.Controls.Add(CreateRepository, 1, 0); Add(root, creation, 42);
            Status = Label(locked ? "Stop active work, clear Emergency Pause, and finish publication or setup before changing connections." : "Not connected - successful features stay local and unpublished", "GitHubStatusLabel"); Status.AccessibleName = null; Status.ForeColor = locked ? Color.FromArgb(160, 92, 12) : Color.DimGray; Add(root, Status, 50);
            FlowLayoutPanel footer = Actions(); footer.FlowDirection = FlowDirection.RightToLeft; Done = Button("Done", "GitHubDoneButton"); Done.DialogResult = DialogResult.OK; footer.Controls.Add(Done); Add(root, footer, 42); AcceptButton = Done; CancelButton = Done;
            foreach (Control control in new Control[] { SignIn, LoadRepositories, UseRepository, CreateRepository, CreateProject, SaveConnection, Disconnect, RepositoryList, Visibility, Repository, BaseBranch, ProjectPath, NewRepository }) control.Enabled = !locked;
        }

        internal void SetRepositories(List<GitHubRepository> repositories)
        {
            RepositoryList.BeginUpdate(); RepositoryList.Items.Clear();
            foreach (GitHubRepository repository in repositories ?? new List<GitHubRepository>()) RepositoryList.Items.Add(new RepositoryChoice(repository));
            if (RepositoryList.Items.Count > 0) RepositoryList.SelectedIndex = 0; RepositoryList.EndUpdate();
        }
        internal GitHubRepository SelectedRepository { get { RepositoryChoice item = RepositoryList.SelectedItem as RepositoryChoice; return item == null ? null : item.Value; } }
        internal void SetProgress(string value) { if (IsDisposed) return; if (InvokeRequired) { try { BeginInvoke(new Action<string>(SetProgress), value); } catch (InvalidOperationException) { } return; } Status.Text = value; }
        internal void ApplyConnection(GitHubResponse response)
        {
            if (response.connection == null) { ConnectionRevision = response.revision; Status.Text = "Not connected - successful features stay local and unpublished"; Disconnect.Enabled = false; return; }
            string project;
            if (!GitHubNative.TryNormalizeProjectPath(response.connection.project, out project))
            {
                Status.Text = "The saved GitHub connection has an invalid local project path."; Status.ForeColor = Color.Firebrick; Disconnect.Enabled = false; return;
            }
            ConnectionRevision = response.revision;
            Repository.Text = response.connection.repository ?? string.Empty; BaseBranch.Text = response.connection.base_branch ?? "main"; ProjectPath.Text = project;
            Status.Text = "Connected - automatic pull request and merge after required checks"; Status.ForeColor = Color.DarkGreen; Disconnect.Enabled = !configurationLocked;
        }
        internal void SetBusy(bool value)
        {
            busy = value;
            foreach (Control control in new Control[] { RefreshAccount, SignIn, LoadRepositories, UseRepository, CreateRepository, CreateProject, SaveConnection, Disconnect }) control.Enabled = !value && !configurationLocked;
            Done.Enabled = !value;
        }
        protected override void OnFormClosing(FormClosingEventArgs e) { if (busy) { e.Cancel = true; Status.Text = "Cancel the active GitHub operation before closing this panel."; return; } base.OnFormClosing(e); }
        private sealed class RepositoryChoice { internal readonly GitHubRepository Value; internal RepositoryChoice(GitHubRepository value) { Value = value; } public override string ToString() { string branch = Value.defaultBranchRef == null ? "no default branch" : Value.defaultBranchRef.name; return Value.nameWithOwner + " · " + Value.visibility + " · " + branch; } }
        private static Label Label(string value, string name) { Label label = new Label(); label.Text = value; label.Name = name; label.AccessibleName = name; label.Dock = DockStyle.Fill; label.TextAlign = ContentAlignment.MiddleLeft; return label; }
        private static TextBox TextBoxFor(string value, string name) { TextBox box = new TextBox(); box.Text = value; box.Name = name; box.AccessibleName = name; box.Dock = DockStyle.Fill; return box; }
        private static ComboBox Combo(string name) { ComboBox box = new ComboBox(); box.Name = name; box.AccessibleName = name; box.Dock = DockStyle.Fill; box.DropDownStyle = ComboBoxStyle.DropDownList; return box; }
        private static Button Button(string value, string name) { Button button = new Button(); button.Text = value; button.Name = name; button.AccessibleName = name; button.AutoSize = true; button.MinimumSize = new Size(105, 31); return button; }
        private static FlowLayoutPanel Actions() { FlowLayoutPanel panel = new FlowLayoutPanel(); panel.Dock = DockStyle.Fill; panel.WrapContents = false; return panel; }
        private static void Add(TableLayoutPanel root, Control control, int height) { root.Controls.Add(control); root.RowStyles.Add(new RowStyle(SizeType.Absolute, height)); }
        private static void AddField(TableLayoutPanel root, string title, string detail, Control control, int height) { TableLayoutPanel holder = new TableLayoutPanel(); holder.Dock = DockStyle.Fill; holder.ColumnCount = 1; holder.RowCount = 3; holder.RowStyles.Add(new RowStyle(SizeType.Absolute, 20)); holder.RowStyles.Add(new RowStyle(SizeType.Absolute, 19)); holder.Controls.Add(Label(title, string.Empty)); Label help = Label(detail, string.Empty); help.ForeColor = Color.DimGray; holder.Controls.Add(help); holder.Controls.Add(control); Add(root, holder, height); }
    }
}
