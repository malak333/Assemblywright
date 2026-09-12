//! Native deterministic Git/GitHub stand-in for the disposable Developer publication E2E.
//! Copy this binary to `git[.exe]` and `gh[.exe]`; never install it as a product tool.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const REAL_GIT: &str = "ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REAL_GIT";
const REMOTE: &str = "ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REMOTE";
const STATE: &str = "ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_STATE";
const SLUG: &str = "ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_SLUG";
const MODE: &str = "ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_MODE";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureRepository {
    id: u64,
    name_with_owner: String,
    visibility: String,
    default_branch: String,
    is_empty: bool,
    can_push: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FixtureState {
    pr_number: Option<u64>,
    head: Option<String>,
    base: Option<String>,
    merged: Option<String>,
    authenticated: bool,
    created_repositories: Vec<FixtureRepository>,
}

fn main() {
    let executable = std::env::current_exe().expect("fixture executable");
    let stem = executable
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("fixture file name")
        .to_ascii_lowercase();
    append_call(&stem, &std::env::args().skip(1).collect::<Vec<_>>());
    let code = if stem == "git" {
        git_fixture()
    } else if stem == "gh" {
        gh_fixture()
    } else {
        panic!("copy developer_github_fixture to git[.exe] or gh[.exe]");
    };
    std::process::exit(code);
}

fn git_fixture() -> i32 {
    let real_git = required_path(REAL_GIT);
    let remote = required_path(REMOTE);
    let slug = required(SLUG);
    let canonical = format!("https://github.com/{slug}.git");
    let arguments: Vec<OsString> = std::env::args_os()
        .skip(1)
        .map(|argument| {
            if argument == OsString::from(&canonical) {
                remote.as_os_str().to_owned()
            } else {
                argument
            }
        })
        .collect();
    let status = Command::new(real_git)
        .args(arguments)
        .env("GIT_CONFIG_VALUE_3", "always")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("real git fixture process");
    status.code().unwrap_or(1)
}

fn gh_fixture() -> i32 {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let slug = required(SLUG);
    let remote = required_path(REMOTE);
    let state_path = required_path(STATE);
    let mut state = load_state(&state_path);
    let mode = fixture_mode();
    match arguments.as_slice() {
        [auth, status, hostname, host]
            if auth == "auth"
                && status == "status"
                && hostname == "--hostname"
                && host == "github.com" =>
        {
            0
        }
        [auth, status, active, hostname, host, json_flag, fields]
            if auth == "auth"
                && status == "status"
                && active == "--active"
                && hostname == "--hostname"
                && host == "github.com"
                && json_flag == "--json"
                && fields == "hosts" =>
        {
            auth_status(&state, &mode)
        }
        [auth, login, hostname, host, protocol, https, web, skip]
            if auth == "auth"
                && login == "login"
                && hostname == "--hostname"
                && host == "github.com"
                && protocol == "--git-protocol"
                && https == "https"
                && web == "--web"
                && skip == "--skip-ssh-key" =>
        {
            assert!(std::env::var_os("GH_TOKEN").is_none());
            assert!(std::env::var_os("GITHUB_TOKEN").is_none());
            let browser = std::env::var("GH_BROWSER").expect("trusted browser sink");
            assert!(browser.contains("true") || browser.to_ascii_lowercase().contains("cmd.exe"));
            sign_in_fixture(&mode, &state_path, &mut state)
        }
        [repo, view, requested, json_flag, fields]
            if repo == "repo"
                && view == "view"
                && requested == &slug
                && json_flag == "--json"
                && fields == "nameWithOwner" =>
        {
            print!("{}", json!({"nameWithOwner": slug}));
            0
        }
        values if values.first().map(String::as_str) == Some("api") => api_fixture(values, &state),
        values if values.starts_with(&["repo".into(), "create".into()]) => {
            create_repository_fixture(values, &mode, &state_path, &mut state)
        }
        values if values.starts_with(&["pr".into(), "list".into()]) => {
            if let (Some(number), Some(head)) = (state.pr_number, state.head.as_deref()) {
                print!(
                    "{}",
                    json!([pr_value(
                        &slug,
                        number,
                        head,
                        state.base.as_deref(),
                        state.merged.as_deref()
                    )])
                );
            } else {
                print!("[]");
            }
            0
        }
        values if values.starts_with(&["pr".into(), "create".into()]) => {
            let branch = option(values, "--head").expect("fixture PR head");
            let reference = format!("refs/heads/{branch}");
            let head = git_output(&remote, &["ls-remote", remote_text(&remote), &reference]);
            let head = head
                .split_whitespace()
                .next()
                .expect("fixture branch commit")
                .to_owned();
            state.pr_number = Some(17);
            state.head = Some(head);
            state.base = Some(
                git_output(
                    &remote,
                    &["ls-remote", remote_text(&remote), "refs/heads/main"],
                )
                .split_whitespace()
                .next()
                .expect("fixture base commit")
                .to_owned(),
            );
            save_state(&state_path, &state);
            print!("https://github.com/{slug}/pull/17");
            0
        }
        values if values.starts_with(&["pr".into(), "view".into()]) => {
            let number = state.pr_number.expect("fixture PR exists");
            let head = state.head.as_deref().expect("fixture PR head");
            print!(
                "{}",
                pr_value(
                    &slug,
                    number,
                    head,
                    state.base.as_deref(),
                    state.merged.as_deref()
                )
            );
            0
        }
        values if values.starts_with(&["pr".into(), "checks".into()]) => {
            if fixture_mode() == "pending" {
                print!(
                    "{}",
                    json!([{"name":"release-local","state":"IN_PROGRESS","bucket":"pending"}])
                );
                return 8;
            }
            print!(
                "{}",
                json!([{"name":"release-local","state":"SUCCESS","bucket":"pass"}])
            );
            0
        }
        values if values.starts_with(&["pr".into(), "merge".into()]) => {
            let expected = option(values, "--match-head-commit").expect("merge head guard");
            let head = state.head.as_deref().expect("fixture PR head");
            assert_eq!(expected, head, "merge must bind the exact head");
            let status = Command::new(required_path(REAL_GIT))
                .args(["--git-dir"])
                .arg(&remote)
                .args(["update-ref", "refs/heads/main", head])
                .status()
                .expect("fixture merge update");
            assert!(status.success(), "fixture merge update failed");
            state.merged = Some(head.to_owned());
            save_state(&state_path, &state);
            0
        }
        _ => panic!("unexpected gh fixture arguments: {arguments:?}"),
    }
}

fn auth_status(state: &FixtureState, mode: &str) -> i32 {
    if mode == "setup-auth-unavailable" {
        print!("{{");
        return 1;
    }
    let ambient_login = fixture_ambient_login();
    let authenticated = ambient_login.is_some()
        || state.authenticated
        || matches!(
            mode,
            "setup-expired"
                | "setup-signed-in"
                | "setup-pages"
                | "setup-create-collision"
                | "setup-create-redirected"
                | "setup-create-ambiguous"
                | "setup-create-saved-then-wait"
                | "setup-create-absent"
                | "setup-create-success"
        );
    let accounts = if authenticated {
        let login = ambient_login.as_deref().unwrap_or("owner");
        json!([{"state":"success","active":true,"host":"github.com","login":login,"tokenSource":"fixture","scopes":"repo","gitProtocol":"https"}])
    } else {
        json!([])
    };
    print!("{}", json!({"hosts":{"github.com":accounts}}));
    0
}

fn sign_in_fixture(mode: &str, state_path: &Path, state: &mut FixtureState) -> i32 {
    if mode == "setup-sign-in-malformed" {
        eprintln!("Use bad-code at https://github.com/login/device");
        return 1;
    }
    if !matches!(
        mode,
        "setup-sign-in-wait" | "setup-sign-in-saved-then-wait" | "setup-sign-in-success"
    ) {
        panic!("unexpected fixture sign-in mode: {mode}");
    }
    eprintln!("First copy your one-time code: ABCD-9XYZ");
    eprintln!("Open https://github.com/login/device in your browser");
    if mode == "setup-sign-in-saved-then-wait" {
        state.authenticated = true;
        save_state(state_path, state);
    }
    if matches!(mode, "setup-sign-in-wait" | "setup-sign-in-saved-then-wait") {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return 1;
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    state.authenticated = true;
    save_state(state_path, state);
    0
}

fn create_repository_fixture(
    arguments: &[String],
    mode: &str,
    state_path: &Path,
    state: &mut FixtureState,
) -> i32 {
    assert_eq!(arguments.len(), 5, "bounded create arguments");
    assert_eq!(arguments[4], "--add-readme");
    assert!(matches!(arguments[3].as_str(), "--private" | "--public"));
    let name_with_owner = &arguments[2];
    let visibility = arguments[3].trim_start_matches("--");
    if mode == "setup-create-collision" {
        panic!("collision must be observed before the create command");
    }
    if mode == "setup-create-absent" {
        return 1;
    }
    if !matches!(
        mode,
        "setup-create-success" | "setup-create-ambiguous" | "setup-create-saved-then-wait"
    ) {
        panic!("unexpected fixture create mode: {mode}");
    }
    let id = match mode {
        "setup-create-success" => 9001,
        "setup-create-ambiguous" => 9002,
        "setup-create-saved-then-wait" => 9003,
        _ => unreachable!(),
    };
    state.created_repositories.retain(|repository| {
        !repository
            .name_with_owner
            .eq_ignore_ascii_case(name_with_owner)
    });
    state.created_repositories.push(FixtureRepository {
        id,
        name_with_owner: name_with_owner.clone(),
        visibility: visibility.into(),
        default_branch: "main".into(),
        is_empty: false,
        can_push: true,
    });
    save_state(state_path, state);
    if mode == "setup-create-saved-then-wait" {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return 1;
    }
    if mode == "setup-create-success" {
        print!("https://github.com/{name_with_owner}");
        0
    } else {
        1
    }
}

fn api_fixture(arguments: &[String], state: &FixtureState) -> i32 {
    let endpoint = arguments.last().expect("fixture api endpoint");
    let mode = fixture_mode();
    if arguments == ["api", "user"] {
        return account_api(&mode, state);
    }
    if endpoint.starts_with("user/repos?") {
        return repository_page_api(endpoint, &mode, state);
    }
    if arguments.len() == 4
        && arguments[0] == "api"
        && arguments[1] == "graphql"
        && arguments[2] == "-f"
        && arguments[3].starts_with("query=")
    {
        return repository_graphql_api(&arguments[3], &mode, state);
    }
    if endpoint.contains("/rules/branches/") {
        let check = if mode == "unbound-policy" {
            json!({"context":"release-local"})
        } else {
            json!({"context":"release-local","integration_id":42})
        };
        let strict = mode != "stale-policy";
        let rules = json!([{"ruleset_id":7,"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":strict,"required_status_checks":[check]}}]);
        if arguments.iter().any(|value| value == "--slurp") {
            print!("{}", json!([rules]));
        } else {
            print!("{rules}");
        }
        return 0;
    }
    if endpoint.ends_with("/rulesets/7") {
        print!(
            "{}",
            json!({"id":7,"target":"branch","enforcement":"active","bypass_actors":[]})
        );
        return 0;
    }
    if endpoint.ends_with("/protection") {
        print!(
            "{}",
            json!({"message":"Branch not protected","status":"404"})
        );
        return 1;
    }
    if endpoint.contains("/check-runs") {
        let commit = endpoint
            .split("/commits/")
            .nth(1)
            .and_then(|value| value.split('/').next())
            .expect("fixture check commit");
        assert_eq!(Some(commit), state.head.as_deref());
        print!(
            "{}",
            json!({"total_count":1,"check_runs":[{"name":"release-local","head_sha":commit,"status":"completed","conclusion":"success","app":{"id":42}}]})
        );
        return 0;
    }
    if let Some(name_with_owner) = endpoint.strip_prefix("repos/") {
        return repository_observation_api(name_with_owner, &mode, state);
    }
    panic!("unexpected gh api endpoint: {endpoint}");
}

fn account_api(mode: &str, state: &FixtureState) -> i32 {
    if mode == "setup-expired" {
        print!("{}", json!({"message":"Bad credentials","status":"401"}));
        return 1;
    }
    if mode == "setup-auth-unavailable" {
        print!(
            "{}",
            json!({"message":"Service unavailable","status":"503"})
        );
        return 1;
    }
    if let Some(login) = fixture_ambient_login().or_else(|| {
        (state.authenticated
            || matches!(
                mode,
                "setup-signed-in"
                    | "setup-pages"
                    | "setup-create-collision"
                    | "setup-create-redirected"
                    | "setup-create-ambiguous"
                    | "setup-create-saved-then-wait"
                    | "setup-create-absent"
                    | "setup-create-success"
            ))
        .then(|| "owner".to_owned())
    }) {
        print!("{}", json!({"login":login,"id":101}));
        return 0;
    }
    print!(
        "{}",
        json!({"message":"Requires authentication","status":"401"})
    );
    1
}

fn repository_page_api(endpoint: &str, mode: &str, state: &FixtureState) -> i32 {
    if mode != "setup-pages" {
        panic!("unexpected repository-page mode: {mode}");
    }
    let page = endpoint
        .split('&')
        .find_map(|part| part.strip_prefix("page="))
        .and_then(|value| value.parse::<u32>().ok())
        .expect("fixture repository page");
    let repositories = match page {
        1 => vec![
            FixtureRepository {
                id: 7001,
                name_with_owner: "owner/active".into(),
                visibility: "public".into(),
                default_branch: "main".into(),
                is_empty: false,
                can_push: true,
            },
            FixtureRepository {
                id: 7002,
                name_with_owner: "owner/empty".into(),
                visibility: "private".into(),
                // REST is known to report `main` for a repository with no branch.
                default_branch: "main".into(),
                is_empty: true,
                can_push: true,
            },
            FixtureRepository {
                id: 7003,
                name_with_owner: "organization/internal".into(),
                visibility: "internal".into(),
                default_branch: "trunk".into(),
                is_empty: false,
                can_push: false,
            },
        ],
        2 => vec![FixtureRepository {
            id: 7010,
            name_with_owner: "owner/second-page".into(),
            visibility: "private".into(),
            default_branch: "main".into(),
            is_empty: false,
            can_push: true,
        }],
        _ => Vec::new(),
    };
    let values: Vec<_> = repositories.iter().map(repository_rest_value).collect();
    let _ = state;
    print!("{}", Value::Array(values));
    0
}

fn repository_observation_api(name_with_owner: &str, mode: &str, state: &FixtureState) -> i32 {
    if mode == "setup-create-absent" {
        print!("{}", json!({"message":"Not Found","status":"404"}));
        return 1;
    }
    let observed = if mode == "setup-create-redirected"
        && name_with_owner.eq_ignore_ascii_case("owner/redirected")
    {
        Some(FixtureRepository {
            id: 8000,
            name_with_owner: "other/transferred".into(),
            visibility: "private".into(),
            default_branch: "main".into(),
            is_empty: false,
            can_push: true,
        })
    } else if mode == "setup-create-collision"
        && name_with_owner.eq_ignore_ascii_case("owner/collision")
    {
        Some(FixtureRepository {
            id: 8001,
            name_with_owner: "owner/collision".into(),
            visibility: "private".into(),
            default_branch: "main".into(),
            is_empty: true,
            can_push: true,
        })
    } else {
        state
            .created_repositories
            .iter()
            .find(|repository| {
                repository
                    .name_with_owner
                    .eq_ignore_ascii_case(name_with_owner)
            })
            .cloned()
    };
    if let Some(repository) = observed {
        print!("{}", repository_rest_value(&repository));
        0
    } else {
        print!("{}", json!({"message":"Not Found","status":"404"}));
        1
    }
}

fn repository_graphql_api(argument: &str, mode: &str, state: &FixtureState) -> i32 {
    let query = argument
        .strip_prefix("query=")
        .expect("fixture GraphQL query");
    let repositories = known_repositories(mode, state);
    let mut data = serde_json::Map::new();
    let mut remainder = query;
    let mut expected_index = 0usize;
    while let Some(alias_start) = remainder.find(&format!("r{expected_index}:repository(owner:\""))
    {
        remainder = &remainder[alias_start..];
        let owner_start = remainder.find("owner:\"").unwrap() + "owner:\"".len();
        let owner_tail = &remainder[owner_start..];
        let owner_end = owner_tail.find('"').expect("fixture GraphQL owner");
        let owner = &owner_tail[..owner_end];
        let name_marker = "name:\"";
        let name_start =
            remainder.find(name_marker).expect("fixture GraphQL name") + name_marker.len();
        let name_tail = &remainder[name_start..];
        let name_end = name_tail.find('"').expect("fixture GraphQL repository");
        let name = &name_tail[..name_end];
        let identity = format!("{owner}/{name}");
        let repository = repositories
            .iter()
            .find(|repository| repository.name_with_owner == identity)
            .unwrap_or_else(|| panic!("unknown fixture GraphQL repository: {identity}"));
        data.insert(
            format!("r{expected_index}"),
            json!({
                "nameWithOwner": repository.name_with_owner,
                "isEmpty": repository.is_empty,
                "defaultBranchRef": if repository.is_empty {
                    Value::Null
                } else {
                    json!({"name":repository.default_branch})
                },
            }),
        );
        expected_index += 1;
        remainder = &remainder[name_start + name_end..];
    }
    assert!(
        expected_index > 0,
        "fixture GraphQL query has no repositories"
    );
    print!("{}", json!({"data":data}));
    0
}

fn known_repositories(mode: &str, state: &FixtureState) -> Vec<FixtureRepository> {
    let mut repositories = state.created_repositories.clone();
    if mode == "setup-pages" {
        repositories.extend([
            FixtureRepository {
                id: 7001,
                name_with_owner: "owner/active".into(),
                visibility: "public".into(),
                default_branch: "main".into(),
                is_empty: false,
                can_push: true,
            },
            FixtureRepository {
                id: 7002,
                name_with_owner: "owner/empty".into(),
                visibility: "private".into(),
                default_branch: "main".into(),
                is_empty: true,
                can_push: true,
            },
            FixtureRepository {
                id: 7003,
                name_with_owner: "organization/internal".into(),
                visibility: "internal".into(),
                default_branch: "trunk".into(),
                is_empty: false,
                can_push: false,
            },
            FixtureRepository {
                id: 7010,
                name_with_owner: "owner/second-page".into(),
                visibility: "private".into(),
                default_branch: "main".into(),
                is_empty: false,
                can_push: true,
            },
        ]);
    }
    if mode == "setup-create-collision" {
        repositories.push(FixtureRepository {
            id: 8001,
            name_with_owner: "owner/collision".into(),
            visibility: "private".into(),
            default_branch: "main".into(),
            is_empty: true,
            can_push: true,
        });
    }
    repositories
}

fn repository_rest_value(repository: &FixtureRepository) -> Value {
    json!({
        "id": repository.id,
        "full_name": repository.name_with_owner,
        "html_url": format!("https://github.com/{}", repository.name_with_owner),
        "visibility": repository.visibility,
        "default_branch": repository.default_branch,
        "permissions": {"push":repository.can_push},
    })
}

fn fixture_ambient_login() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"].into_iter().find_map(|name| {
        (std::env::var(name).as_deref() == Ok("fixture-ambient-token"))
            .then(|| "ambient".to_owned())
    })
}

fn pr_value(
    slug: &str,
    number: u64,
    head: &str,
    base: Option<&str>,
    merged: Option<&str>,
) -> Value {
    json!({
        "number": number,
        "url": format!("https://github.com/{slug}/pull/{number}"),
        "headRefOid": head,
        "baseRefName": "main",
        "baseRefOid": merged.or(base),
        "state": if merged.is_some() { "MERGED" } else { "OPEN" },
        "mergeCommit": merged.map(|oid| json!({"oid":oid})),
    })
}

fn option<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn git_output(remote: &Path, arguments: &[&str]) -> String {
    let output = Command::new(required_path(REAL_GIT))
        .args(arguments)
        .current_dir(remote.parent().unwrap_or(remote))
        .output()
        .expect("real git output");
    assert!(output.status.success(), "real git command failed");
    String::from_utf8(output.stdout).expect("git UTF-8")
}

fn remote_text(remote: &Path) -> &str {
    remote.to_str().expect("fixture remote path")
}

fn load_state(path: &Path) -> FixtureState {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).expect("fixture state"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FixtureState::default(),
        Err(error) => panic!("fixture state read: {error}"),
    }
}

fn save_state(path: &Path, state: &FixtureState) {
    std::fs::write(path, serde_json::to_vec(state).unwrap()).expect("fixture state write");
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(required(name))
}

fn fixture_mode() -> String {
    std::env::var_os(MODE)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| "pass".into())
        .trim()
        .to_owned()
}

fn append_call(tool: &str, arguments: &[String]) {
    use std::io::Write as _;
    let path = required_path(STATE).with_extension("calls");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("fixture call evidence");
    writeln!(file, "{}", json!({"tool":tool,"arguments":arguments})).expect("fixture call write");
}
