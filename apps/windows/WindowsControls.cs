using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Linq;
using System.Windows.Forms;

namespace Assemblywright.Windows
{
    internal enum HeaderIconKind { Gear, Shield, GitHub }

    internal sealed class ChatGptModelChoice
    {
        internal ChatGptModelChoice(string id, string displayName, bool advertised)
        {
            Id = id ?? string.Empty;
            DisplayName = displayName ?? string.Empty;
            Advertised = advertised;
        }

        internal string Id { get; private set; }
        internal string DisplayName { get; private set; }
        internal bool Advertised { get; private set; }
        public override string ToString() { return Advertised ? DisplayName : Id + " (unavailable)"; }
    }

    internal sealed class ReasoningChoice
    {
        internal ReasoningChoice(string value, bool advertised)
        {
            Value = value ?? string.Empty;
            Advertised = advertised;
        }

        internal string Value { get; private set; }
        internal bool Advertised { get; private set; }
        public override string ToString()
        {
            string label = Value == "xhigh" ? "Extra high" : (Value.Length == 0 ? string.Empty : char.ToUpperInvariant(Value[0]) + Value.Substring(1));
            return Advertised ? label : label + " (unavailable)";
        }
    }

    internal sealed class HeaderIconButton : Button
    {
        private readonly HeaderIconKind kind;

        internal HeaderIconButton(HeaderIconKind kind, string automationId, string accessibleName)
        {
            this.kind = kind;
            Name = automationId;
            AccessibleName = accessibleName;
            Size = new Size(38, 38);
            FlatStyle = FlatStyle.Flat;
            FlatAppearance.BorderSize = 0;
            FlatAppearance.MouseOverBackColor = Color.FromArgb(49, 65, 88);
            FlatAppearance.MouseDownBackColor = Color.FromArgb(63, 82, 108);
            BackColor = Color.FromArgb(27, 37, 54);
            ForeColor = Color.White;
            TabStop = true;
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            Color color = Enabled ? Color.White : Color.FromArgb(130, 142, 158);
            using (Pen pen = new Pen(color, 1.9F))
            using (SolidBrush brush = new SolidBrush(color))
            {
                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                if (kind == HeaderIconKind.GitHub) PaintBranch(e.Graphics, pen, brush);
                else if (kind == HeaderIconKind.Shield) PaintHalfShield(e.Graphics, pen, brush);
                else PaintGearShape(e.Graphics, pen);
            }
        }

        // Vector equivalents of the Mac SF Symbols arrow.triangle.branch,
        // shield.lefthalf.filled, and gearshape.
        private static void PaintBranch(Graphics graphics, Pen pen, SolidBrush brush)
        {
            PointF top = new PointF(19F, 8F), left = new PointF(10F, 27F), right = new PointF(28F, 27F);
            graphics.DrawLine(pen, top, left); graphics.DrawLine(pen, top, right); graphics.DrawLine(pen, left, right);
            graphics.DrawLine(pen, top, new PointF(19F, 24F));
            graphics.DrawLine(pen, new PointF(19F, 24F), new PointF(15.5F, 20.5F));
            graphics.DrawLine(pen, new PointF(19F, 24F), new PointF(22.5F, 20.5F));
            graphics.FillEllipse(brush, 6.8F, 23.8F, 6.4F, 6.4F);
            graphics.FillEllipse(brush, 24.8F, 23.8F, 6.4F, 6.4F);
            graphics.FillEllipse(brush, 15.8F, 4.8F, 6.4F, 6.4F);
        }

        private static void PaintHalfShield(Graphics graphics, Pen pen, SolidBrush brush)
        {
            PointF[] shield = new PointF[] { new PointF(19F, 5.5F), new PointF(29F, 9.5F), new PointF(28F, 21F), new PointF(24.5F, 27F), new PointF(19F, 31.5F), new PointF(13.5F, 27F), new PointF(10F, 21F), new PointF(9F, 9.5F) };
            using (GraphicsPath path = new GraphicsPath())
            {
                path.AddPolygon(shield);
                using (Region left = new Region(new RectangleF(7F, 4F, 12F, 30F)))
                {
                    left.Intersect(path); graphics.FillRegion(brush, left);
                }
                graphics.DrawPath(pen, path);
            }
            graphics.DrawLine(pen, new PointF(19F, 6F), new PointF(19F, 31F));
        }

        private static void PaintGearShape(Graphics graphics, Pen pen)
        {
            PointF center = new PointF(19F, 19F);
            PointF[] points = new PointF[24];
            for (int index = 0; index < points.Length; index++)
            {
                double angle = -Math.PI / 2.0 + index * Math.PI * 2.0 / points.Length;
                float radius = index % 3 == 1 ? 13F : (index % 3 == 2 ? 10.3F : 11F);
                points[index] = new PointF(center.X + (float)Math.Cos(angle) * radius, center.Y + (float)Math.Sin(angle) * radius);
            }
            graphics.DrawPolygon(pen, points); graphics.DrawEllipse(pen, 14.5F, 14.5F, 9F, 9F);
        }
    }

    internal sealed class SettingsDialog : Form
    {
        private readonly TextBox endpointBox;
        private readonly ComboBox localModelBox, localReasoningBox, orchestratorModelBox, orchestratorReasoningBox, reviewerModelBox, reviewerReasoningBox;
        private readonly NumericUpDown attemptsBox, timeoutBox;
        private readonly Dictionary<string, LocalModelOption> localModels;
        private readonly Dictionary<string, ChatGptModelOption> chatGptModels;
        private readonly GuiSettings original;
        private readonly bool locked;
        private readonly Button save;

        internal SettingsDialog(GuiSettings settings, bool locked, IList<LocalModelOption> localCatalog, IList<ChatGptModelOption> chatGptCatalog)
        {
            localModels = (localCatalog ?? new List<LocalModelOption>()).ToDictionary(delegate(LocalModelOption item) { return item.Id; }, StringComparer.Ordinal);
            chatGptModels = (chatGptCatalog ?? new List<ChatGptModelOption>()).ToDictionary(delegate(ChatGptModelOption item) { return item.Id; }, StringComparer.Ordinal);
            original = settings; this.locked = locked;
            Name = "SettingsDialog"; AccessibleName = "Settings"; Text = "Settings";
            FormBorderStyle = FormBorderStyle.FixedDialog; MaximizeBox = false; MinimizeBox = false; ShowInTaskbar = false; StartPosition = FormStartPosition.CenterParent;
            ClientSize = new Size(650, 760); Font = new Font("Segoe UI", 9F);
            TableLayoutPanel layout = new TableLayoutPanel(); layout.Dock = DockStyle.Fill; layout.Padding = new Padding(22); layout.ColumnCount = 1; Controls.Add(layout);
            Label title = LabelFor("Settings"); title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold); Add(layout, title, 38);
            Add(layout, LabelFor("ChatGPT can plan and review. Local AI remains the only implementation model."), 38);
            endpointBox = AddText(layout, "Local implementation endpoint", "SettingsEndpointBox", settings.endpoint, 50);
            localModelBox = AddCombo(layout, "Local implementation model", "Writes code inside the bounded local pipeline.", "SettingsLocalModelBox", settings.model, localModels.Keys.ToList(), false);
            localReasoningBox = AddReasoning(layout, "Local implementation reasoning", "SettingsLocalReasoningBox", settings.reasoning_effort, LocalEfforts(settings.model), "none");
            orchestratorModelBox = AddCombo(layout, "ChatGPT orchestrator", "Plans in chat and prepares feature specifications.", "SettingsOrchestratorModelBox", settings.orchestrator_model, new List<string>(), true);
            orchestratorReasoningBox = AddReasoning(layout, "Orchestrator reasoning", "SettingsOrchestratorReasoningBox", settings.orchestrator_reasoning_effort, new string[0], string.Empty);
            reviewerModelBox = AddCombo(layout, "ChatGPT reviewer", "Reviews candidate code and validation evidence.", "SettingsReviewerModelBox", settings.reviewer_model, new List<string>(), true);
            reviewerReasoningBox = AddReasoning(layout, "Reviewer reasoning", "SettingsReviewerReasoningBox", settings.reviewer_reasoning_effort, new string[0], string.Empty);
            SelectChatGptRole(orchestratorModelBox, orchestratorReasoningBox, settings.orchestrator_provider == RoleProvider.ChatGpt ? settings.orchestrator_model : string.Empty, settings.orchestrator_reasoning_effort);
            SelectChatGptRole(reviewerModelBox, reviewerReasoningBox, settings.reviewer_provider == RoleProvider.ChatGpt ? settings.reviewer_model : string.Empty, settings.reviewer_reasoning_effort);
            attemptsBox = Number("SettingsAttemptsBox", 1, 20, settings.default_attempts); timeoutBox = Number("SettingsTimeoutBox", 1, 3600, settings.timeout_seconds);
            TableLayoutPanel numbers = new TableLayoutPanel(); numbers.Dock = DockStyle.Fill; numbers.ColumnCount = 4;
            numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 34)); numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 16)); numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 34)); numbers.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 16));
            numbers.Controls.Add(LabelFor("Default attempts"), 0, 0); numbers.Controls.Add(attemptsBox, 1, 0); numbers.Controls.Add(LabelFor("Timeout seconds"), 2, 0); numbers.Controls.Add(timeoutBox, 3, 0); Add(layout, numbers, 38);
            Label note = LabelFor(locked ? "Wait for active work, chat, publication, setup, or Emergency Pause to finish before saving settings." : "ChatGPT choices come from the signed-in ChatGPT account. Reload refreshes both catalogs."); note.ForeColor = locked ? Color.FromArgb(160, 92, 12) : Color.DimGray; Add(layout, note, 38);
            FlowLayoutPanel actions = new FlowLayoutPanel(); actions.Dock = DockStyle.Fill; actions.FlowDirection = FlowDirection.RightToLeft;
            save = ButtonFor("Save", "SettingsSaveButton"); save.DialogResult = DialogResult.OK; save.Enabled = false;
            Button cancel = ButtonFor("Cancel", "SettingsCancelButton"); cancel.DialogResult = DialogResult.Cancel;
            Button reload = ButtonFor("Reload models", "SettingsReloadButton"); reload.DialogResult = DialogResult.Retry;
            actions.Controls.Add(save); actions.Controls.Add(cancel); actions.Controls.Add(reload); Add(layout, actions, 42);
            foreach (Control control in new Control[] { endpointBox, localModelBox, localReasoningBox, orchestratorModelBox, orchestratorReasoningBox, reviewerModelBox, reviewerReasoningBox, attemptsBox, timeoutBox }) control.Enabled = !locked;
            endpointBox.TextChanged += delegate { UpdateSave(); }; localModelBox.TextChanged += delegate { ReplaceEfforts(localReasoningBox, LocalEfforts(localModelBox.Text), "none"); UpdateSave(); };
            orchestratorModelBox.SelectedIndexChanged += delegate { ReplaceChatGptEfforts(orchestratorModelBox, orchestratorReasoningBox); UpdateSave(); };
            reviewerModelBox.SelectedIndexChanged += delegate { ReplaceChatGptEfforts(reviewerModelBox, reviewerReasoningBox); UpdateSave(); };
            localReasoningBox.SelectedIndexChanged += delegate { UpdateSave(); }; orchestratorReasoningBox.SelectedIndexChanged += delegate { UpdateSave(); }; reviewerReasoningBox.SelectedIndexChanged += delegate { UpdateSave(); }; attemptsBox.ValueChanged += delegate { UpdateSave(); }; timeoutBox.ValueChanged += delegate { UpdateSave(); }; UpdateSave();
            AcceptButton = save; CancelButton = cancel;
        }

        private void UpdateSave()
        {
            save.Enabled = !locked && SelectionsValid() && (endpointBox.Text.Trim() != original.endpoint || localModelBox.Text.Trim() != original.model || Convert.ToString(localReasoningBox.SelectedItem) != original.reasoning_effort
                || original.orchestrator_provider != RoleProvider.ChatGpt || SelectedModelId(orchestratorModelBox) != original.orchestrator_model || SelectedReasoning(orchestratorReasoningBox) != original.orchestrator_reasoning_effort
                || original.reviewer_provider != RoleProvider.ChatGpt || SelectedModelId(reviewerModelBox) != original.reviewer_model || SelectedReasoning(reviewerReasoningBox) != original.reviewer_reasoning_effort
                || Decimal.ToInt32(attemptsBox.Value) != original.default_attempts || Decimal.ToInt32(timeoutBox.Value) != original.timeout_seconds);
        }

        private bool SelectionsValid()
        {
            return RolePairValid(SelectedModelId(orchestratorModelBox), SelectedReasoning(orchestratorReasoningBox))
                && RolePairValid(SelectedModelId(reviewerModelBox), SelectedReasoning(reviewerReasoningBox));
        }

        private bool RolePairValid(string model, string effort)
        {
            ChatGptModelOption option; return chatGptModels.TryGetValue(model, out option) && option.ReasoningEfforts.Contains(effort);
        }

        internal GuiSettings ReadSettings(GuiSettings current, bool incrementRevision)
        {
            GuiSettings value = new GuiSettings(); value.endpoint = endpointBox.Text.Trim(); value.model = localModelBox.Text.Trim(); value.reasoning_effort = Convert.ToString(localReasoningBox.SelectedItem);
            value.orchestrator_provider = RoleProvider.ChatGpt; value.orchestrator_model = SelectedModelId(orchestratorModelBox); value.orchestrator_reasoning_effort = SelectedReasoning(orchestratorReasoningBox);
            value.reviewer_provider = RoleProvider.ChatGpt; value.reviewer_model = SelectedModelId(reviewerModelBox); value.reviewer_reasoning_effort = SelectedReasoning(reviewerReasoningBox);
            value.default_attempts = Decimal.ToInt32(attemptsBox.Value); value.timeout_seconds = Decimal.ToInt32(timeoutBox.Value); value.revision = incrementRevision ? current.revision + 1 : current.revision; return value;
        }

        private void SelectChatGptRole(ComboBox modelBox, ComboBox reasoningBox, string requestedModel, string requestedEffort)
        {
            List<ChatGptModelChoice> values = chatGptModels.Values.Select(delegate(ChatGptModelOption option) { return new ChatGptModelChoice(option.Id, option.Name, true); }).ToList();
            modelBox.BeginUpdate(); modelBox.Items.Clear(); foreach (ChatGptModelChoice value in values) modelBox.Items.Add(value);
            ChatGptModelChoice selected = values.FirstOrDefault(delegate(ChatGptModelChoice value) { return value.Id == requestedModel; });
            if (selected == null && !string.IsNullOrWhiteSpace(requestedModel)) { selected = new ChatGptModelChoice(requestedModel, requestedModel, false); modelBox.Items.Add(selected); }
            if (selected == null && values.Count != 0) selected = values[0];
            modelBox.SelectedItem = selected; modelBox.EndUpdate();
            ReplaceChatGptEfforts(modelBox, reasoningBox, requestedEffort);
        }
        private void ReplaceChatGptEfforts(ComboBox modelBox, ComboBox reasoningBox) { ReplaceChatGptEfforts(modelBox, reasoningBox, SelectedReasoning(reasoningBox), false); }
        private void ReplaceChatGptEfforts(ComboBox modelBox, ComboBox reasoningBox, string requested)
        {
            ReplaceChatGptEfforts(modelBox, reasoningBox, requested, true);
        }
        private void ReplaceChatGptEfforts(ComboBox modelBox, ComboBox reasoningBox, string requested, bool retainUnavailable)
        {
            ChatGptModelOption option; IEnumerable<string> efforts = chatGptModels.TryGetValue(SelectedModelId(modelBox), out option) ? (IEnumerable<string>)option.ReasoningEfforts : new string[0];
            string fallback = option == null ? string.Empty : option.DefaultReasoningEffort; ReplaceChatGptReasoning(reasoningBox, efforts, requested, fallback, retainUnavailable);
        }
        internal static string SelectedModelId(ComboBox box) { ChatGptModelChoice choice = box.SelectedItem as ChatGptModelChoice; return choice == null ? string.Empty : choice.Id; }
        internal static string SelectedReasoning(ComboBox box) { ReasoningChoice choice = box.SelectedItem as ReasoningChoice; return choice == null ? string.Empty : choice.Value; }
        private static void ReplaceChatGptReasoning(ComboBox box, IEnumerable<string> values, string requested, string fallback, bool retainUnavailable)
        {
            List<ReasoningChoice> choices = values.Select(delegate(string value) { return new ReasoningChoice(value, true); }).ToList();
            ReasoningChoice selected = choices.FirstOrDefault(delegate(ReasoningChoice choice) { return choice.Value == requested; });
            if (selected == null && retainUnavailable && !string.IsNullOrWhiteSpace(requested)) { selected = new ReasoningChoice(requested, false); choices.Add(selected); }
            if (selected == null) selected = choices.FirstOrDefault(delegate(ReasoningChoice choice) { return choice.Value == fallback; }) ?? choices.FirstOrDefault();
            box.BeginUpdate(); box.Items.Clear(); foreach (ReasoningChoice choice in choices) box.Items.Add(choice); box.SelectedItem = selected; box.EndUpdate();
        }
        private IEnumerable<string> LocalEfforts(string model) { LocalModelOption option; return localModels.TryGetValue(model ?? string.Empty, out option) ? option.AvailableEfforts : ReasoningEffort.Values; }
        private static ComboBox AddCombo(TableLayoutPanel layout, string label, string detail, string name, string current, IList<string> values, bool listOnly)
        {
            ComboBox box = new ComboBox(); box.Name = name; box.AccessibleName = name; box.Dock = DockStyle.Fill; box.DropDownStyle = listOnly ? ComboBoxStyle.DropDownList : ComboBoxStyle.DropDown;
            foreach (string item in (values ?? new List<string>()).Where(delegate(string value) { return !string.IsNullOrWhiteSpace(value); }).Distinct(StringComparer.Ordinal)) box.Items.Add(item);
            if (!listOnly && !string.IsNullOrWhiteSpace(current) && !box.Items.Contains(current)) box.Items.Add(current); if (box.Items.Contains(current)) box.SelectedItem = current; else box.Text = current ?? string.Empty;
            AddLabeled(layout, label, detail, box, string.IsNullOrEmpty(detail) ? 50 : 66); return box;
        }
        private static void ReplaceEfforts(ComboBox box, IEnumerable<string> values, string requested) { ReplaceEfforts(box, values, requested, string.Empty); }
        private static void ReplaceEfforts(ComboBox box, IEnumerable<string> values, string requested, string fallback)
        {
            box.BeginUpdate(); box.Items.Clear(); foreach (string value in values) box.Items.Add(value); string selected = box.Items.Contains(requested) ? requested : (box.Items.Contains(fallback) ? fallback : (box.Items.Count == 0 ? null : Convert.ToString(box.Items[0]))); box.SelectedItem = selected; box.EndUpdate();
        }
        private static ComboBox AddReasoning(TableLayoutPanel layout, string label, string name, string current, IEnumerable<string> efforts, string fallback)
        {
            ComboBox box = new ComboBox(); box.Name = name; box.AccessibleName = name; box.Dock = DockStyle.Fill; box.DropDownStyle = ComboBoxStyle.DropDownList;
            ReplaceEfforts(box, efforts, current, fallback); AddLabeled(layout, label, string.Empty, box, 50); return box;
        }
        private static TextBox AddText(TableLayoutPanel layout, string label, string name, string value, int height) { TextBox box = new TextBox(); box.Name = name; box.AccessibleName = name; box.Dock = DockStyle.Fill; box.Text = value ?? string.Empty; AddLabeled(layout, label, string.Empty, box, height); return box; }
        private static void AddLabeled(TableLayoutPanel layout, string label, string detail, Control control, int height)
        {
            TableLayoutPanel holder = new TableLayoutPanel(); holder.Dock = DockStyle.Fill; holder.ColumnCount = 1; holder.RowCount = string.IsNullOrEmpty(detail) ? 2 : 3; holder.RowStyles.Add(new RowStyle(SizeType.Absolute, 20)); holder.Controls.Add(LabelFor(label));
            if (!string.IsNullOrEmpty(detail)) { Label d = LabelFor(detail); d.ForeColor = Color.DimGray; holder.Controls.Add(d); holder.RowStyles.Add(new RowStyle(SizeType.Absolute, 18)); }
            holder.Controls.Add(control); Add(layout, holder, height);
        }
        private static void Add(TableLayoutPanel layout, Control control, int height) { layout.Controls.Add(control); layout.RowStyles.Add(new RowStyle(SizeType.Absolute, height)); }
        private static Label LabelFor(string text) { Label label = new Label(); label.Text = text; label.Dock = DockStyle.Fill; label.TextAlign = ContentAlignment.MiddleLeft; return label; }
        private static NumericUpDown Number(string name, int min, int max, int value) { NumericUpDown number = new NumericUpDown(); number.Name = name; number.AccessibleName = name; number.Minimum = min; number.Maximum = max; number.Value = Math.Max(min, Math.Min(max, value)); number.Dock = DockStyle.Fill; return number; }
        internal static Button ButtonFor(string text, string name) { Button button = new Button(); button.Text = text; button.Name = name; button.AccessibleName = name; button.AutoSize = true; button.MinimumSize = new Size(104, 32); return button; }
    }

    internal sealed class PermissionsDialog : Form
    {
        private readonly RadioButton ask, auto, full;
        private readonly Button save;
        private readonly string originalMode;
        private readonly bool locked;
        internal PermissionsDialog(PermissionsDocument permissions, bool locked)
        {
            this.locked = locked; originalMode = permissions.mode;
            Name = "PermissionsDialog"; AccessibleName = "Permissions"; Text = "Permissions";
            FormBorderStyle = FormBorderStyle.FixedDialog; MaximizeBox = false; MinimizeBox = false; ShowInTaskbar = false; StartPosition = FormStartPosition.CenterParent; ClientSize = new Size(530, 440); Font = new Font("Segoe UI", 9F);
            TableLayoutPanel layout = new TableLayoutPanel(); layout.Dock = DockStyle.Fill; layout.Padding = new Padding(22); layout.ColumnCount = 1; Controls.Add(layout);
            Label title = LabelFor("Permissions"); title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold); Add(layout, title, 40);
            Add(layout, LabelFor("This setting applies to every project, new chat, and new feature on Windows."), 38);
            ask = AddMode(layout, "Ask for approval", "Ask before commands, file changes, installations, or internet access.", "PermissionsAskRadio", permissions.mode == "ask");
            auto = AddMode(layout, "Approve for me", "Allow project edits and web searches. Ask before commands, installations, URL fetches, or outside-project access.", "PermissionsAutoRadio", permissions.mode == "auto");
            full = AddMode(layout, "Full access", "Allow project tools, internet access, and files available to the Windows account.", "PermissionsFullRadio", permissions.mode == "full");
            Label state = LabelFor("Windows execution host · revision " + permissions.revision + (locked ? "\r\nStop active work, clear Emergency Pause, and finish publication or setup before saving." : string.Empty)); state.Name = "PermissionsStatusLabel"; state.ForeColor = locked ? Color.FromArgb(160, 92, 12) : Color.DimGray; Add(layout, state, 48);
            FlowLayoutPanel actions = new FlowLayoutPanel(); actions.Dock = DockStyle.Fill; actions.FlowDirection = FlowDirection.RightToLeft;
            save = SettingsDialog.ButtonFor("Save", "PermissionsSaveButton"); save.DialogResult = DialogResult.OK; save.Enabled = false;
            Button done = SettingsDialog.ButtonFor("Done", "PermissionsCloseButton"); done.DialogResult = DialogResult.Cancel;
            Button reload = SettingsDialog.ButtonFor("Reload", "PermissionsReloadButton"); reload.DialogResult = DialogResult.Retry;
            actions.Controls.Add(save); actions.Controls.Add(done); actions.Controls.Add(reload); Add(layout, actions, 44); AcceptButton = save; CancelButton = done;
            ask.CheckedChanged += delegate { UpdateSave(); }; auto.CheckedChanged += delegate { UpdateSave(); }; full.CheckedChanged += delegate { UpdateSave(); }; UpdateSave();
        }
        internal string SelectedMode { get { return ask.Checked ? "ask" : (auto.Checked ? "auto" : "full"); } }
        private void UpdateSave() { save.Enabled = !locked && SelectedMode != originalMode; }
        private static RadioButton AddMode(TableLayoutPanel layout, string title, string help, string name, bool selected) { RadioButton button = new RadioButton(); button.Text = title + Environment.NewLine + help; button.Name = name; button.AccessibleName = title; button.Checked = selected; button.Dock = DockStyle.Fill; button.AutoSize = false; button.Padding = new Padding(6); Add(layout, button, 68); return button; }
        private static void Add(TableLayoutPanel layout, Control control, int height) { layout.Controls.Add(control); layout.RowStyles.Add(new RowStyle(SizeType.Absolute, height)); }
        private static Label LabelFor(string text) { Label label = new Label(); label.Text = text; label.Dock = DockStyle.Fill; label.TextAlign = ContentAlignment.MiddleLeft; return label; }
    }

    internal sealed class ApprovalDialog : Form
    {
        internal ApprovalDialog(ApprovalDocument approval)
        {
            Name = "ApprovalDialog"; AccessibleName = "Approval required"; Text = "Approval required";
            FormBorderStyle = FormBorderStyle.FixedDialog; MaximizeBox = false; MinimizeBox = false; ShowInTaskbar = false; StartPosition = FormStartPosition.CenterParent; ClientSize = new Size(620, 410); Font = new Font("Segoe UI", 9F);
            TableLayoutPanel layout = new TableLayoutPanel(); layout.Dock = DockStyle.Fill; layout.Padding = new Padding(22); layout.ColumnCount = 1; Controls.Add(layout);
            Label title = new Label(); title.Text = approval.action == "run_validation" ? "Allow validation command?" : "Allow proposed file changes?"; title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold); title.Dock = DockStyle.Fill; Add(layout, title, 42);
            Label detail = new Label(); detail.Name = "ApprovalDetailLabel"; detail.AccessibleName = null; detail.Text = (approval.detail ?? string.Empty) + Environment.NewLine + Environment.NewLine
                + "Run: " + approval.run_id + Environment.NewLine + "Action: " + approval.action + Environment.NewLine + "Digest: " + approval.digest + Environment.NewLine
                + "Approval revision: " + approval.revision + Environment.NewLine + "Approval token: " + approval.token; detail.Dock = DockStyle.Fill; detail.AutoEllipsis = true; detail.TextAlign = ContentAlignment.TopLeft; Add(layout, detail, 220);
            Label boundary = new Label(); boundary.Text = "This decision applies only to this exact pending request. Rejecting stops the run."; boundary.ForeColor = Color.DimGray; boundary.Dock = DockStyle.Fill; Add(layout, boundary, 40);
            FlowLayoutPanel actions = new FlowLayoutPanel(); actions.Dock = DockStyle.Fill; actions.FlowDirection = FlowDirection.RightToLeft;
            Button approve = SettingsDialog.ButtonFor("Approve", "ApprovalApproveButton"); approve.DialogResult = DialogResult.Yes;
            Button reject = SettingsDialog.ButtonFor("Reject", "ApprovalRejectButton"); reject.DialogResult = DialogResult.No;
            actions.Controls.Add(approve); actions.Controls.Add(reject); Add(layout, actions, 42); AcceptButton = approve; CancelButton = reject;
        }
        private static void Add(TableLayoutPanel layout, Control control, int height) { layout.Controls.Add(control); layout.RowStyles.Add(new RowStyle(SizeType.Absolute, height)); }
    }

    internal sealed class ReviewerDialog : Form
    {
        private readonly ComboBox model, reasoning;
        private readonly IList<ChatGptModelOption> chatGptModels;
        private readonly Button save;
        internal ReviewerDialog(JobDocument job, IList<ChatGptModelOption> chatGptModels)
        {
            this.chatGptModels = chatGptModels ?? new List<ChatGptModelOption>();
            Name = "ReviewerDialog"; AccessibleName = "Change reviewer"; Text = "Change reviewer"; FormBorderStyle = FormBorderStyle.FixedDialog; MaximizeBox = false; MinimizeBox = false; StartPosition = FormStartPosition.CenterParent; ClientSize = new Size(520, 285); Font = new Font("Segoe UI", 9F);
            TableLayoutPanel layout = new TableLayoutPanel(); layout.Dock = DockStyle.Fill; layout.Padding = new Padding(22); layout.ColumnCount = 1; Controls.Add(layout);
            Label title = new Label(); title.Text = "Change reviewer"; title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold); title.Dock = DockStyle.Fill; Add(layout, title, 42);
            Label help = new Label(); help.Text = "Choose the model and reasoning level for this feature's next review. Saving does not start the feature."; help.Dock = DockStyle.Fill; Add(layout, help, 48);
            model = new ComboBox(); model.Name = "FeatureReviewerModelBox"; model.AccessibleName = model.Name; model.Dock = DockStyle.Fill; model.DropDownStyle = ComboBoxStyle.DropDownList; Add(layout, model, 36);
            reasoning = new ComboBox(); reasoning.Name = "FeatureReviewerReasoningBox"; reasoning.AccessibleName = reasoning.Name; reasoning.Dock = DockStyle.Fill; reasoning.DropDownStyle = ComboBoxStyle.DropDownList; Add(layout, reasoning, 36);
            string current = job.reviewer_provider == RoleProvider.ChatGpt ? job.reviewer_model : string.Empty; PopulateModels(current, job.reviewer_reasoning_effort);
            model.SelectedIndexChanged += delegate { PopulateEfforts(SettingsDialog.SelectedReasoning(reasoning), false); UpdateSave(); }; reasoning.SelectedIndexChanged += delegate { UpdateSave(); };
            FlowLayoutPanel actions = new FlowLayoutPanel(); actions.Dock = DockStyle.Fill; actions.FlowDirection = FlowDirection.RightToLeft; save = SettingsDialog.ButtonFor("Save reviewer", "FeatureReviewerSaveButton"); save.DialogResult = DialogResult.OK; Button cancel = SettingsDialog.ButtonFor("Cancel", "FeatureReviewerCancelButton"); cancel.DialogResult = DialogResult.Cancel; actions.Controls.Add(save); actions.Controls.Add(cancel); Add(layout, actions, 42); AcceptButton = save; CancelButton = cancel; UpdateSave();
        }
        private void PopulateModels(string requestedModel, string requestedEffort)
        {
            List<ChatGptModelChoice> values = chatGptModels.Select(delegate(ChatGptModelOption item) { return new ChatGptModelChoice(item.Id, item.Name, true); }).ToList();
            ChatGptModelChoice selected = values.FirstOrDefault(delegate(ChatGptModelChoice value) { return value.Id == requestedModel; });
            if (selected == null && !string.IsNullOrWhiteSpace(requestedModel)) { selected = new ChatGptModelChoice(requestedModel, requestedModel, false); values.Add(selected); }
            if (selected == null && values.Count != 0) selected = values[0];
            model.BeginUpdate(); model.Items.Clear(); foreach (ChatGptModelChoice value in values) model.Items.Add(value); model.SelectedItem = selected; model.EndUpdate(); PopulateEfforts(requestedEffort, true);
        }
        private void PopulateEfforts(string requested, bool retainUnavailable)
        {
            ChatGptModelOption option = chatGptModels.FirstOrDefault(delegate(ChatGptModelOption item) { return item.Id == SettingsDialog.SelectedModelId(model); }); IEnumerable<string> efforts = option == null ? (IEnumerable<string>)new string[0] : option.ReasoningEfforts; string fallback = option == null ? string.Empty : option.DefaultReasoningEffort;
            List<ReasoningChoice> values = efforts.Select(delegate(string value) { return new ReasoningChoice(value, true); }).ToList(); ReasoningChoice selected = values.FirstOrDefault(delegate(ReasoningChoice value) { return value.Value == requested; });
            if (selected == null && retainUnavailable && !string.IsNullOrWhiteSpace(requested)) { selected = new ReasoningChoice(requested, false); values.Add(selected); }
            if (selected == null) selected = values.FirstOrDefault(delegate(ReasoningChoice value) { return value.Value == fallback; }) ?? values.FirstOrDefault();
            reasoning.BeginUpdate(); reasoning.Items.Clear(); foreach (ReasoningChoice value in values) reasoning.Items.Add(value); reasoning.SelectedItem = selected; reasoning.EndUpdate();
        }
        private void UpdateSave() { ChatGptModelOption option = chatGptModels.FirstOrDefault(delegate(ChatGptModelOption item) { return item.Id == SettingsDialog.SelectedModelId(model); }); save.Enabled = option != null && option.ReasoningEfforts.Contains(SettingsDialog.SelectedReasoning(reasoning)); }
        internal void Apply(JobDocument job) { job.reviewer_provider = RoleProvider.ChatGpt; job.reviewer_model = SettingsDialog.SelectedModelId(model); job.reviewer_reasoning_effort = SettingsDialog.SelectedReasoning(reasoning); }
        private static void Add(TableLayoutPanel layout, Control control, int height) { layout.Controls.Add(control); layout.RowStyles.Add(new RowStyle(SizeType.Absolute, height)); }
    }
}
