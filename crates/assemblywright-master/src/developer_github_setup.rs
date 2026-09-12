//! Durable public metadata for Developer GitHub account and repository setup.
//!
//! Authentication secrets and device codes never enter this state. Windows keeps
//! credentials in GitHub CLI's configured credential storage; this module retains
//! only the operation identities and external-effect evidence needed for recovery.

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

const SCHEMA_VERSION: u8 = 1;
const CREATION_HISTORY_LIMIT: usize = 40;
pub(super) const DEVICE_VERIFICATION_URL: &str = "https://github.com/login/device";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountRecord {
    pub state: String,
    pub login: Option<String>,
    pub message: String,
}

impl Default for AccountRecord {
    fn default() -> Self {
        Self {
            state: "unknown".into(),
            login: None,
            message: "Refresh GitHub account status to continue".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct RepositoryRecord {
    pub name_with_owner: String,
    pub url: String,
    pub visibility: String,
    pub default_branch: String,
    pub can_push: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SignInRecord {
    pub operation_id: String,
    pub state: String,
    pub message: String,
    #[serde(skip)]
    pub user_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CreationRecord {
    pub operation_id: String,
    pub repository_url: Option<String>,
    pub repository_id: Option<u64>,
    pub name_with_owner: String,
    pub visibility: String,
    pub default_branch: Option<String>,
    pub state: String,
    pub message: String,
    pub preflight_absent: bool,
    pub command_succeeded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct GithubSetupState {
    schema_version: u8,
    pub account: AccountRecord,
    pub repositories: Vec<RepositoryRecord>,
    pub repository_page: u32,
    pub has_more: bool,
    pub sign_in: Option<SignInRecord>,
    #[serde(default)]
    sign_in_operation_ids: Vec<String>,
    creations: Vec<CreationRecord>,
    active_creation_id: Option<String>,
}

impl Default for GithubSetupState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            account: AccountRecord::default(),
            repositories: Vec::new(),
            repository_page: 0,
            has_more: false,
            sign_in: None,
            sign_in_operation_ids: Vec::new(),
            creations: Vec::new(),
            active_creation_id: None,
        }
    }
}

impl GithubSetupState {
    pub(super) fn initialize_and_load(connection: &Connection) -> Result<Self> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS developer_github_setup(\
             id INTEGER PRIMARY KEY CHECK(id=1), state TEXT NOT NULL);",
        )?;
        let encoded = connection
            .query_row(
                "SELECT state FROM developer_github_setup WHERE id=1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let mut state = match encoded.as_ref() {
            Some(encoded) => serde_json::from_str(encoded)
                .context("Persisted Developer GitHub setup state is invalid")?,
            None => Self::default(),
        };
        let normalized = state.normalize_after_restart();
        state.validate()?;
        if normalized || encoded.is_none() {
            Self::persist(connection, &state)?;
        }
        Ok(state)
    }

    pub(super) fn persist(connection: &Connection, state: &Self) -> Result<()> {
        state.validate()?;
        let encoded = serde_json::to_string(state)?;
        connection.execute(
            "INSERT INTO developer_github_setup(id,state) VALUES(1,?1) \
             ON CONFLICT(id) DO UPDATE SET state=excluded.state",
            [encoded],
        )?;
        Ok(())
    }

    pub(super) fn persist_in(transaction: &Transaction<'_>, state: &Self) -> Result<()> {
        state.validate()?;
        let encoded = serde_json::to_string(state)?;
        transaction.execute(
            "INSERT INTO developer_github_setup(id,state) VALUES(1,?1) \
             ON CONFLICT(id) DO UPDATE SET state=excluded.state",
            [encoded],
        )?;
        Ok(())
    }

    pub(super) fn snapshot(&self, revision: u64, busy: bool, can_mutate: bool) -> Value {
        let sign_in = self.sign_in.as_ref().map(|record| {
            json!({
                "operation_id": record.operation_id,
                "user_code": record.user_code,
                "verification_url": record.user_code.as_ref().map(|_| DEVICE_VERIFICATION_URL),
                "state": record.state,
                "message": record.message,
            })
        });
        let creation = self.active_creation().map(|record| {
            json!({
                "operation_id": record.operation_id,
                "repository_url": record.repository_url,
                "repository_id": record.repository_id.map(|id| id.to_string()),
                "name_with_owner": record.name_with_owner,
                "visibility": record.visibility,
                "default_branch": record.default_branch,
                "state": record.state,
                "message": record.message,
            })
        });
        json!({
            "revision": revision,
            "account": self.account,
            "repositories": self.repositories,
            "repository_page": self.repository_page,
            "has_more": self.has_more,
            "sign_in": sign_in,
            "creation": creation,
            "busy": busy,
            "can_mutate": can_mutate,
        })
    }

    pub(super) fn reset_repositories_if_account_changed(&mut self, next: &AccountRecord) {
        if self.account.state != next.state || self.account.login != next.login {
            self.repositories.clear();
            self.repository_page = 0;
            self.has_more = false;
        }
        self.account = next.clone();
    }

    pub(super) fn begin_sign_in(&mut self, operation_id: &str) -> Result<()> {
        validate_operation_id(operation_id)?;
        if self
            .sign_in_operation_ids
            .iter()
            .any(|retained| retained == operation_id)
        {
            bail!("GitHub sign-in operation ID was already used");
        }
        if self.sign_in.as_ref().is_some_and(|record| {
            matches!(record.state.as_str(), "starting" | "waiting" | "attention")
        }) {
            bail!("Reconcile the retained GitHub sign-in before starting another");
        }
        if self.sign_in_operation_ids.len() >= CREATION_HISTORY_LIMIT {
            bail!("GitHub sign-in operation history limit reached");
        }
        self.sign_in_operation_ids.push(operation_id.into());
        self.sign_in = Some(SignInRecord {
            operation_id: operation_id.into(),
            state: "starting".into(),
            message: "Starting GitHub device sign-in".into(),
            user_code: None,
        });
        Ok(())
    }

    pub(super) fn record_sign_in_challenge(
        &mut self,
        operation_id: &str,
        user_code: &str,
    ) -> Result<()> {
        validate_device_code(user_code)?;
        let record = self.sign_in_mut(operation_id)?;
        if record.state != "starting" {
            bail!("GitHub sign-in challenge arrived in an invalid state");
        }
        record.state = "waiting".into();
        record.message = "Enter this one-time code on GitHub to finish signing in".into();
        record.user_code = Some(user_code.into());
        Ok(())
    }

    pub(super) fn finish_sign_in(
        &mut self,
        operation_id: &str,
        state: &str,
        message: &str,
        account: &AccountRecord,
    ) -> Result<()> {
        if !matches!(state, "succeeded" | "cancelled" | "failed" | "attention") {
            bail!("GitHub sign-in completion state is invalid");
        }
        let record = self.sign_in_mut(operation_id)?;
        record.state = state.into();
        record.message = bounded_message(message)?;
        record.user_code = None;
        self.reset_repositories_if_account_changed(account);
        Ok(())
    }

    pub(super) fn sign_in_mut(&mut self, operation_id: &str) -> Result<&mut SignInRecord> {
        validate_operation_id(operation_id)?;
        self.sign_in
            .as_mut()
            .filter(|record| record.operation_id == operation_id)
            .context("GitHub sign-in operation does not match the retained operation")
    }

    pub(super) fn start_creation(
        &mut self,
        operation_id: &str,
        expected_login: &str,
        name: &str,
        visibility: &str,
    ) -> Result<CreationStart> {
        validate_operation_id(operation_id)?;
        validate_login(expected_login)?;
        validate_repository_name(name)?;
        validate_creation_visibility(visibility)?;
        if let Some(existing) = self
            .creations
            .iter()
            .find(|record| record.operation_id == operation_id)
        {
            if !existing
                .name_with_owner
                .eq_ignore_ascii_case(&format!("{expected_login}/{name}"))
                || existing.visibility != visibility
            {
                bail!("GitHub creation operation ID was reused with different contents");
            }
            self.active_creation_id = Some(operation_id.into());
            return Ok(CreationStart::ExistingOperation);
        }
        if self
            .creations
            .iter()
            .any(|record| matches!(record.state.as_str(), "creating" | "attention"))
        {
            bail!("Reconcile the unfinished GitHub repository creation first");
        }
        if self.creations.len() >= CREATION_HISTORY_LIMIT {
            bail!("GitHub repository creation history limit reached");
        }
        self.creations.push(CreationRecord {
            operation_id: operation_id.into(),
            repository_url: None,
            repository_id: None,
            name_with_owner: format!("{expected_login}/{name}"),
            visibility: visibility.into(),
            default_branch: None,
            state: "creating".into(),
            message: "Verifying the exact GitHub repository target before creation".into(),
            preflight_absent: false,
            command_succeeded: false,
        });
        self.active_creation_id = Some(operation_id.into());
        Ok(CreationStart::NewOperation)
    }

    pub(super) fn creation_mut(&mut self, operation_id: &str) -> Result<&mut CreationRecord> {
        validate_operation_id(operation_id)?;
        self.active_creation_id = Some(operation_id.into());
        self.creations
            .iter_mut()
            .find(|record| record.operation_id == operation_id)
            .context("GitHub repository creation operation was not retained")
    }

    pub(super) fn active_creation(&self) -> Option<&CreationRecord> {
        let operation_id = self.active_creation_id.as_deref()?;
        self.creations
            .iter()
            .find(|record| record.operation_id == operation_id)
    }

    pub(super) fn blocks_dependent_work(&self) -> bool {
        self.sign_in.as_ref().is_some_and(|record| {
            matches!(record.state.as_str(), "starting" | "waiting" | "attention")
        }) || self
            .creations
            .iter()
            .any(|record| matches!(record.state.as_str(), "creating" | "attention"))
    }

    fn normalize_after_restart(&mut self) -> bool {
        let mut changed = false;
        if let Some(operation_id) = self
            .sign_in
            .as_ref()
            .map(|record| record.operation_id.clone())
            .filter(|operation_id| !self.sign_in_operation_ids.contains(operation_id))
        {
            self.sign_in_operation_ids.push(operation_id);
            changed = true;
        }
        if let Some(sign_in) = self
            .sign_in
            .as_mut()
            .filter(|record| matches!(record.state.as_str(), "starting" | "waiting"))
        {
            sign_in.state = "attention".into();
            sign_in.message =
                "GitHub sign-in was interrupted; reconcile the retained operation".into();
            sign_in.user_code = None;
            changed = true;
        }
        for creation in self
            .creations
            .iter_mut()
            .filter(|record| record.state == "creating")
        {
            creation.state = "attention".into();
            creation.message =
                "Repository creation was interrupted; reconcile by observation before retrying"
                    .into();
            changed = true;
        }
        changed
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            bail!("Persisted Developer GitHub setup version is unsupported");
        }
        validate_account(&self.account)?;
        if self.repositories.len() > 50 || self.repository_page > 100 {
            bail!("Persisted GitHub repository page exceeds its bound");
        }
        for repository in &self.repositories {
            validate_repository(repository)?;
        }
        if self.repository_page == 0 && (!self.repositories.is_empty() || self.has_more) {
            bail!("Persisted GitHub repository page evidence is invalid");
        }
        if let Some(sign_in) = &self.sign_in {
            validate_operation_id(&sign_in.operation_id)?;
            if !matches!(
                sign_in.state.as_str(),
                "starting" | "waiting" | "succeeded" | "cancelled" | "failed" | "attention"
            ) || sign_in.message.is_empty()
                || sign_in.message.len() > 1000
            {
                bail!("Persisted GitHub sign-in evidence is invalid");
            }
            if sign_in
                .user_code
                .as_deref()
                .is_some_and(|code| validate_device_code(code).is_err())
            {
                bail!("Transient GitHub sign-in challenge is invalid");
            }
        }
        let mut sign_in_ids = std::collections::BTreeSet::new();
        for operation_id in &self.sign_in_operation_ids {
            validate_operation_id(operation_id)?;
            if !sign_in_ids.insert(operation_id) {
                bail!("Persisted GitHub sign-in operation is duplicated");
            }
        }
        if self.sign_in_operation_ids.len() > CREATION_HISTORY_LIMIT
            || self
                .sign_in
                .as_ref()
                .is_some_and(|record| !sign_in_ids.contains(&record.operation_id))
        {
            bail!("Persisted GitHub sign-in operation history is invalid");
        }
        if self.creations.len() > CREATION_HISTORY_LIMIT {
            bail!("Persisted GitHub creation history exceeds its bound");
        }
        let mut operation_ids = std::collections::BTreeSet::new();
        for creation in &self.creations {
            validate_creation(creation)?;
            if !operation_ids.insert(&creation.operation_id) {
                bail!("Persisted GitHub creation operation is duplicated");
            }
        }
        if self
            .active_creation_id
            .as_ref()
            .is_some_and(|id| !operation_ids.contains(id))
        {
            bail!("Persisted active GitHub creation operation is missing");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CreationStart {
    NewOperation,
    ExistingOperation,
}

pub(super) fn validate_operation_id(operation_id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(operation_id).context("GitHub operation ID is invalid")?;
    if parsed.to_string() != operation_id {
        bail!("GitHub operation ID must use canonical UUID text");
    }
    Ok(())
}

pub(super) fn validate_login(login: &str) -> Result<()> {
    if login.is_empty()
        || login.len() > 39
        || login.starts_with('-')
        || login.ends_with('-')
        || !login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("GitHub account login is invalid");
    }
    Ok(())
}

pub(super) fn validate_repository_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 100
        || matches!(name, "." | "..")
        || name.to_ascii_lowercase().ends_with(".git")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("Use a simple GitHub repository name");
    }
    Ok(())
}

pub(super) fn validate_creation_visibility(visibility: &str) -> Result<()> {
    if !matches!(visibility, "public" | "private") {
        bail!("GitHub repository visibility must be public or private");
    }
    Ok(())
}

pub(super) fn validate_device_code(code: &str) -> Result<()> {
    let bytes = code.as_bytes();
    if bytes.len() != 9
        || bytes[4] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && !byte.is_ascii_uppercase() && !byte.is_ascii_digit())
    {
        bail!("GitHub device challenge is invalid");
    }
    Ok(())
}

fn bounded_message(message: &str) -> Result<String> {
    if message.is_empty() || message.len() > 1000 || message.chars().any(char::is_control) {
        bail!("GitHub setup message is invalid");
    }
    Ok(message.into())
}

fn validate_account(account: &AccountRecord) -> Result<()> {
    if !matches!(
        account.state.as_str(),
        "unknown" | "signed_out" | "signed_in" | "unavailable"
    ) || account.message.is_empty()
        || account.message.len() > 1000
    {
        bail!("Persisted GitHub account state is invalid");
    }
    match (&*account.state, account.login.as_deref()) {
        ("signed_in", Some(login)) => validate_login(login),
        ("signed_in", None) => bail!("Signed-in GitHub account has no login"),
        (_, None) => Ok(()),
        (_, Some(_)) => bail!("Unsigned GitHub account cannot retain a login"),
    }
}

fn validate_repository(repository: &RepositoryRecord) -> Result<()> {
    let (owner, name) = split_name_with_owner(&repository.name_with_owner)?;
    let expected = format!("https://github.com/{owner}/{name}");
    if repository.url != expected
        || !matches!(
            repository.visibility.as_str(),
            "public" | "private" | "internal"
        )
        || repository.default_branch.len() > 255
        || repository.default_branch.chars().any(char::is_control)
    {
        bail!("Persisted GitHub repository observation is invalid");
    }
    Ok(())
}

fn validate_creation(creation: &CreationRecord) -> Result<()> {
    validate_operation_id(&creation.operation_id)?;
    let (owner, name) = split_name_with_owner(&creation.name_with_owner)?;
    validate_creation_visibility(&creation.visibility)?;
    if !matches!(
        creation.state.as_str(),
        "creating" | "succeeded" | "attention" | "existing" | "absent"
    ) || creation.message.is_empty()
        || creation.message.len() > 1000
    {
        bail!("Persisted GitHub creation state is invalid");
    }
    if let Some(url) = &creation.repository_url {
        if url != &format!("https://github.com/{owner}/{name}") {
            bail!("Persisted GitHub creation URL changed identity");
        }
    }
    if let Some(branch) = creation
        .default_branch
        .as_deref()
        .filter(|branch| !branch.is_empty())
    {
        validate_default_branch(branch)?;
    }
    if creation.repository_id == Some(0)
        || creation.state == "succeeded"
            && (!creation.preflight_absent
                || !creation.command_succeeded
                || creation.repository_url.is_none()
                || creation.repository_id.is_none()
                || creation.default_branch.as_deref().is_none_or(str::is_empty))
        || matches!(creation.state.as_str(), "existing" | "attention")
            && creation.repository_id.is_some()
            && creation.repository_url.is_none()
        || creation.state == "absent"
            && (creation.repository_url.is_some()
                || creation.repository_id.is_some()
                || creation.default_branch.is_some())
    {
        bail!("Persisted GitHub creation evidence is incomplete");
    }
    Ok(())
}

fn validate_default_branch(branch: &str) -> Result<()> {
    if branch.len() > 120
        || branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("@{")
        || branch.ends_with('.')
        || branch.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
    {
        bail!("Persisted GitHub default branch is invalid");
    }
    Ok(())
}

fn split_name_with_owner(value: &str) -> Result<(&str, &str)> {
    let mut parts = value.split('/');
    let owner = parts.next().context("GitHub repository owner is missing")?;
    let name = parts.next().context("GitHub repository name is missing")?;
    if parts.next().is_some() {
        bail!("GitHub repository identity is invalid");
    }
    validate_login(owner)?;
    validate_repository_name(name)?;
    Ok((owner, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_and_creation_inputs_are_exact() {
        assert!(validate_device_code("ABCD-9XYZ").is_ok());
        for bad in ["abcd-9XYZ", "ABCDE-XYZ", "ABCD_9XYZ", "ABCD-XYZ!"] {
            assert!(validate_device_code(bad).is_err(), "{bad}");
        }
        assert!(validate_repository_name("inches-feet_demo.2").is_ok());
        for bad in ["", "../repo", "owner/repo", "repo.git"] {
            assert!(validate_repository_name(bad).is_err(), "{bad}");
        }
        assert!(validate_creation_visibility("internal").is_err());
    }

    #[test]
    fn restart_keeps_public_operation_metadata_but_never_device_code() {
        let mut state = GithubSetupState::default();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        state.begin_sign_in(operation).unwrap();
        state
            .record_sign_in_challenge(operation, "ABCD-9XYZ")
            .unwrap();
        let encoded = serde_json::to_string(&state).unwrap();
        assert!(!encoded.contains("ABCD-9XYZ"));
        assert!(!encoded.contains(DEVICE_VERIFICATION_URL));
        let mut restored: GithubSetupState = serde_json::from_str(&encoded).unwrap();
        assert!(restored.normalize_after_restart());
        let sign_in = restored.sign_in.unwrap();
        assert_eq!(sign_in.operation_id, operation);
        assert_eq!(sign_in.state, "attention");
        assert!(sign_in.user_code.is_none());
    }

    #[test]
    fn succeeded_creation_requires_exact_effect_receipt() {
        let mut state = GithubSetupState::default();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        assert_eq!(
            state
                .start_creation(operation, "owner", "repo", "private")
                .unwrap(),
            CreationStart::NewOperation
        );
        let creation = state.creation_mut(operation).unwrap();
        creation.state = "succeeded".into();
        assert!(state.validate().is_err());
        let creation = state.creation_mut(operation).unwrap();
        creation.preflight_absent = true;
        creation.command_succeeded = true;
        creation.repository_url = Some("https://github.com/owner/repo".into());
        creation.repository_id = Some(42);
        creation.default_branch = Some("main".into());
        creation.message = "Repository creation was verified".into();
        assert!(state.validate().is_ok());
    }

    #[test]
    fn unknown_persisted_states_fail_closed() {
        let mut value = serde_json::to_value(GithubSetupState::default()).unwrap();
        value["schema_version"] = json!(2);
        let state: GithubSetupState = serde_json::from_value(value).unwrap();
        assert!(state.validate().is_err());
    }

    #[test]
    fn sign_in_operation_ids_cannot_replay() {
        let mut state = GithubSetupState::default();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        state.begin_sign_in(operation).unwrap();
        state
            .finish_sign_in(
                operation,
                "cancelled",
                "GitHub sign-in was cancelled",
                &AccountRecord {
                    state: "signed_out".into(),
                    login: None,
                    message: "Sign in to GitHub to continue".into(),
                },
            )
            .unwrap();
        assert!(state.begin_sign_in(operation).is_err());
    }

    #[test]
    fn legacy_schema_one_sign_in_is_migrated_before_validation() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE developer_github_setup(\
                 id INTEGER PRIMARY KEY CHECK(id=1), state TEXT NOT NULL);",
            )
            .unwrap();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        let mut value = serde_json::to_value(GithubSetupState::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("sign_in_operation_ids");
        value["sign_in"] = json!({
            "operation_id": operation,
            "state": "waiting",
            "message": "Enter the device code"
        });
        connection
            .execute(
                "INSERT INTO developer_github_setup(id,state) VALUES(1,?1)",
                [serde_json::to_string(&value).unwrap()],
            )
            .unwrap();

        let mut restored = GithubSetupState::initialize_and_load(&connection).unwrap();
        assert_eq!(restored.sign_in.as_ref().unwrap().state, "attention");
        assert!(restored.begin_sign_in(operation).is_err());
    }

    #[test]
    fn creation_snapshot_serializes_repository_id_as_decimal_text() {
        let mut state = GithubSetupState::default();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        state
            .start_creation(operation, "owner", "repo", "public")
            .unwrap();
        state.creation_mut(operation).unwrap().repository_id = Some(42);
        assert_eq!(
            state.snapshot(8, false, true)["creation"]["repository_id"],
            "42"
        );
    }

    #[test]
    fn existing_empty_repository_receipt_is_valid_but_success_requires_branch() {
        let mut state = GithubSetupState::default();
        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        state
            .start_creation(operation, "owner", "empty", "private")
            .unwrap();
        let creation = state.creation_mut(operation).unwrap();
        creation.state = "existing".into();
        creation.repository_url = Some("https://github.com/owner/empty".into());
        creation.repository_id = Some(42);
        creation.default_branch = Some(String::new());
        creation.message = "The repository already exists and is empty".into();
        assert!(state.validate().is_ok());

        state.creation_mut(operation).unwrap().state = "succeeded".into();
        assert!(state.validate().is_err());
    }
}
