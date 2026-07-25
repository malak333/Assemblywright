use crate::{JarvisError, JarvisResult, PluginActionManifest, PluginManifest, PluginSource};
use rustix::fs::{fstat, open, Mode, OFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::path::Path;
use std::time::{Duration, Instant};
use wasmi::{
    Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TypedResumableCall,
};

pub const MAX_WASM_MODULE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WASM_REQUEST_BYTES: usize = 256 * 1024;
pub const MAX_WASM_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_WASM_MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WASM_TABLE_ELEMENTS: usize = 0;
pub const MAX_WASM_FUEL: u64 = 10_000_000;
const WASM_FUEL_SLICE: u64 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmControlState {
    Continue,
    EmergencyPaused,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPluginExecution {
    pub output: Value,
    pub request_bytes: usize,
    pub output_bytes: usize,
    pub fuel_consumed: u64,
}

#[derive(Debug)]
pub struct WasmArtifact {
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub fn read_wasm_artifact(path: &Path) -> JarvisResult<WasmArtifact> {
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| JarvisError::Validation("WASM module could not be opened safely".to_string()))?;
    let stat = fstat(&fd)
        .map_err(|_| JarvisError::Validation("WASM module metadata unavailable".to_string()))?;
    let size = usize::try_from(stat.st_size)
        .map_err(|_| JarvisError::Validation("WASM module size is invalid".to_string()))?;
    if size == 0 || size > MAX_WASM_MODULE_BYTES {
        return Err(JarvisError::Validation(format!(
            "WASM module must be between 1 and {MAX_WASM_MODULE_BYTES} bytes"
        )));
    }
    let raw_fd = std::os::fd::IntoRawFd::into_raw_fd(fd);
    // SAFETY: ownership was transferred from rustix's OwnedFd above.
    let file = unsafe { File::from_raw_fd(raw_fd) };
    let mut bytes = Vec::with_capacity(size);
    file.take((MAX_WASM_MODULE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| JarvisError::Validation("WASM module could not be read".to_string()))?;
    if bytes.len() != size {
        return Err(JarvisError::Validation(
            "WASM module changed while it was being read".to_string(),
        ));
    }
    let digest = Sha256::digest(&bytes);
    Ok(WasmArtifact {
        bytes,
        sha256: format!("{digest:x}"),
    })
}

pub fn execute_installed_wasm_plugin(
    manifest: &PluginManifest,
    action: &PluginActionManifest,
    module_bytes: &[u8],
    input: &Value,
    mut control: impl FnMut() -> WasmControlState,
) -> JarvisResult<WasmPluginExecution> {
    if manifest.source != PluginSource::LocalWasm {
        return Err(JarvisError::Validation(
            "installed WASM execution requires local_wasm source".to_string(),
        ));
    }
    if module_bytes.is_empty() || module_bytes.len() > MAX_WASM_MODULE_BYTES {
        return Err(JarvisError::Validation(
            "WASM module size is invalid".to_string(),
        ));
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(action.timeout.timeout_ms))
        .ok_or_else(|| JarvisError::Validation("WASM timeout is invalid".to_string()))?;
    ensure_runtime(deadline, &mut control)?;
    let request = serde_json::to_vec(input)
        .map_err(|_| JarvisError::Plugin("serialize WASM input".to_string()))?;
    if request.len() > MAX_WASM_REQUEST_BYTES {
        return Err(JarvisError::Validation(format!(
            "WASM request exceeds {MAX_WASM_REQUEST_BYTES} byte limit"
        )));
    }
    ensure_runtime(deadline, &mut control)?;

    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, module_bytes)
        .map_err(|_| JarvisError::Plugin("WASM module validation failed".to_string()))?;
    ensure_runtime(deadline, &mut control)?;
    if module.imports().next().is_some() {
        return Err(JarvisError::PolicyBlocked(
            "WASM imports are forbidden (including WASI)".to_string(),
        ));
    }
    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_WASM_MEMORY_BYTES)
        .memories(1)
        .instances(1)
        .tables(0)
        .table_elements(MAX_WASM_TABLE_ELEMENTS)
        .build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits: &mut StoreLimits| limits);
    store
        .set_fuel(WASM_FUEL_SLICE)
        .map_err(|_| JarvisError::Plugin("initialize WASM fuel".to_string()))?;
    let linker = Linker::<StoreLimits>::new(&engine);
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| JarvisError::Plugin("WASM instantiation failed".to_string()))?;
    ensure_runtime(deadline, &mut control)?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| JarvisError::Validation("WASM export memory is required".to_string()))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, "jarvis_alloc")
        .map_err(|_| {
            JarvisError::Validation("WASM export jarvis_alloc(i32)->i32 is required".to_string())
        })?;
    let run = instance
        .get_typed_func::<(i32, i32), i64>(&store, "jarvis_run")
        .map_err(|_| {
            JarvisError::Validation("WASM export jarvis_run(i32,i32)->i64 is required".to_string())
        })?;
    ensure_runtime(deadline, &mut control)?;
    let mut fuel_budget = WASM_FUEL_SLICE;
    let input_ptr = drive_call_i32(
        alloc.call_resumable(&mut store, request.len() as i32),
        &mut store,
        deadline,
        &mut fuel_budget,
        &mut control,
    )?;
    let input_offset = usize::try_from(input_ptr)
        .map_err(|_| JarvisError::Plugin("WASM allocator returned invalid pointer".to_string()))?;
    memory
        .write(&mut store, input_offset, &request)
        .map_err(|_| JarvisError::Plugin("WASM input memory range is invalid".to_string()))?;
    ensure_runtime(deadline, &mut control)?;
    let packed = drive_call_i64(
        run.call_resumable(&mut store, (input_ptr, request.len() as i32)),
        &mut store,
        deadline,
        &mut fuel_budget,
        &mut control,
    )?;
    ensure_runtime(deadline, &mut control)?;
    let packed = packed as u64;
    let output_ptr = (packed >> 32) as usize;
    let output_len = (packed & u32::MAX as u64) as usize;
    if output_len > MAX_WASM_OUTPUT_BYTES {
        return Err(JarvisError::Plugin(format!(
            "WASM output exceeds {MAX_WASM_OUTPUT_BYTES} byte limit"
        )));
    }
    let mut output_bytes = vec![0; output_len];
    memory
        .read(&store, output_ptr, &mut output_bytes)
        .map_err(|_| JarvisError::Plugin("WASM output memory range is invalid".to_string()))?;
    ensure_runtime(deadline, &mut control)?;
    let output: Value = serde_json::from_slice(&output_bytes)
        .map_err(|_| JarvisError::Plugin("WASM output is not valid JSON".to_string()))?;
    action
        .output_schema
        .validate_value(&format!("{}.{} output", manifest.id, action.name), &output)?;
    ensure_runtime(deadline, &mut control)?;
    Ok(WasmPluginExecution {
        output,
        request_bytes: request.len(),
        output_bytes: output_len,
        fuel_consumed: fuel_budget.saturating_sub(store.get_fuel().unwrap_or(0)),
    })
}

fn drive_call_i32(
    initial: Result<TypedResumableCall<i32>, wasmi::Error>,
    store: &mut Store<StoreLimits>,
    deadline: Instant,
    fuel_budget: &mut u64,
    control: &mut impl FnMut() -> WasmControlState,
) -> JarvisResult<i32> {
    let mut call =
        initial.map_err(|_| JarvisError::Plugin("WASM execution trapped".to_string()))?;
    loop {
        match call {
            TypedResumableCall::Finished(value) => {
                ensure_runtime(deadline, control)?;
                return Ok(value);
            }
            TypedResumableCall::HostTrap(_) => {
                return Err(JarvisError::Plugin("WASM host trap".to_string()))
            }
            TypedResumableCall::OutOfFuel(resumable) => {
                replenish(store, deadline, fuel_budget, control)?;
                call = resumable
                    .resume(&mut *store)
                    .map_err(|_| JarvisError::Plugin("WASM execution trapped".to_string()))?;
            }
        }
    }
}

fn drive_call_i64(
    initial: Result<TypedResumableCall<i64>, wasmi::Error>,
    store: &mut Store<StoreLimits>,
    deadline: Instant,
    fuel_budget: &mut u64,
    control: &mut impl FnMut() -> WasmControlState,
) -> JarvisResult<i64> {
    let mut call =
        initial.map_err(|_| JarvisError::Plugin("WASM execution trapped".to_string()))?;
    loop {
        match call {
            TypedResumableCall::Finished(value) => {
                ensure_runtime(deadline, control)?;
                return Ok(value);
            }
            TypedResumableCall::HostTrap(_) => {
                return Err(JarvisError::Plugin("WASM host trap".to_string()))
            }
            TypedResumableCall::OutOfFuel(resumable) => {
                replenish(store, deadline, fuel_budget, control)?;
                call = resumable
                    .resume(&mut *store)
                    .map_err(|_| JarvisError::Plugin("WASM execution trapped".to_string()))?;
            }
        }
    }
}

fn replenish(
    store: &mut Store<StoreLimits>,
    deadline: Instant,
    fuel_budget: &mut u64,
    control: &mut impl FnMut() -> WasmControlState,
) -> JarvisResult<()> {
    ensure_runtime(deadline, control)?;
    if *fuel_budget >= MAX_WASM_FUEL {
        return Err(JarvisError::Plugin("WASM fuel limit exhausted".to_string()));
    }
    let next = WASM_FUEL_SLICE.min(MAX_WASM_FUEL - *fuel_budget);
    *fuel_budget += next;
    store
        .set_fuel(next)
        .map_err(|_| JarvisError::Plugin("replenish WASM fuel".to_string()))
}

fn ensure_runtime(
    deadline: Instant,
    control: &mut impl FnMut() -> WasmControlState,
) -> JarvisResult<()> {
    ensure_control(control())?;
    if Instant::now() >= deadline {
        return Err(JarvisError::Plugin("WASM execution timed out".to_string()));
    }
    Ok(())
}

fn ensure_control(state: WasmControlState) -> JarvisResult<()> {
    match state {
        WasmControlState::Continue => Ok(()),
        WasmControlState::EmergencyPaused => Err(JarvisError::PolicyBlocked(
            "WASM execution cancelled by emergency pause".to_string(),
        )),
        WasmControlState::Cancelled => {
            Err(JarvisError::Plugin("WASM execution cancelled".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CancellationBehavior, JsonSchema, PluginAccess, PluginNetworkAccess, PluginPermission,
        PluginTimeout, PluginTimeoutAction, PluginWasmAbi, PluginWasmManifest, RiskTier,
    };
    use serde_json::{json, Map};

    fn manifest() -> PluginManifest {
        let mut properties = Map::new();
        properties.insert("ok".to_string(), json!({"type": "boolean"}));
        PluginManifest {
            manifest_schema_version: 1,
            id: "wasm_fixture".to_string(),
            name: "WASM fixture".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::LocalWasm,
            author: "Assemblywright Test".to_string(),
            source_path: None,
            subprocess: None,
            wasm: Some(PluginWasmManifest {
                module: "plugin.wasm".to_string(),
                abi: PluginWasmAbi::JarvisJsonV1,
            }),
            publisher_signature: None,
            actions: vec![PluginActionManifest {
                name: "run".to_string(),
                description: "Compute a bounded result.".to_string(),
                permissions: Vec::<PluginPermission>::new(),
                risk_tier: RiskTier::Low,
                input_schema: JsonSchema::empty_object(),
                output_schema: JsonSchema::object(properties, vec!["ok".to_string()]),
                proactive: false,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                network_access: PluginNetworkAccess::default(),
                audit_fields: Vec::new(),
                timeout: PluginTimeout {
                    timeout_ms: 1_000,
                    on_timeout: PluginTimeoutAction::Cancel,
                },
                cancellation: CancellationBehavior::Cooperative,
            }],
        }
    }

    fn module_returning(output: &str) -> Vec<u8> {
        let ptr = 1024_u64;
        let packed = (ptr << 32) | output.len() as u64;
        wat::parse_str(format!(
            r#"(module
                (memory (export "memory") 1)
                (data (i32.const 1024) "{}")
                (func (export "jarvis_alloc") (param i32) (result i32)
                    i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64)
                    i64.const {packed}))"#,
            output.replace('\\', "\\\\").replace('"', "\\\"")
        ))
        .expect("valid WAT")
    }

    #[test]
    fn confined_module_returns_schema_valid_json() {
        let manifest = manifest();
        let execution = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &module_returning(r#"{"ok":true}"#),
            &json!({}),
            || WasmControlState::Continue,
        )
        .expect("confined execution");
        assert_eq!(execution.output, json!({"ok": true}));
        assert!(execution.fuel_consumed <= MAX_WASM_FUEL);
        assert!(execution.output_bytes <= MAX_WASM_OUTPUT_BYTES);
    }

    #[test]
    fn imports_wasi_resources_and_oversized_output_fail_closed() {
        let manifest = manifest();
        let imported = wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_write" (func))
                (memory (export "memory") 1)
                (func (export "jarvis_alloc") (param i32) (result i32) i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64) i64.const 0))"#,
        )
        .unwrap();
        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &imported,
            &json!({}),
            || WasmControlState::Continue,
        )
        .expect_err("WASI import must fail");
        assert!(error.to_string().contains("imports are forbidden"));

        let oversized_memory = wat::parse_str(
            r#"(module
                (memory (export "memory") 257)
                (func (export "jarvis_alloc") (param i32) (result i32) i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64) i64.const 0))"#,
        )
        .unwrap();
        assert!(execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &oversized_memory,
            &json!({}),
            || WasmControlState::Continue,
        )
        .is_err());

        let oversized_table = wat::parse_str(
            r#"(module
                (table 1000000 funcref)
                (memory (export "memory") 1)
                (func (export "jarvis_alloc") (param i32) (result i32) i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64) i64.const 0))"#,
        )
        .unwrap();
        assert!(execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &oversized_table,
            &json!({}),
            || WasmControlState::Continue,
        )
        .is_err());

        let oversized_output = wat::parse_str(format!(
            r#"(module
                (memory (export "memory") 17)
                (func (export "jarvis_alloc") (param i32) (result i32) i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64)
                    i64.const {}))"#,
            MAX_WASM_OUTPUT_BYTES + 1
        ))
        .unwrap();
        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &oversized_output,
            &json!({}),
            || WasmControlState::Continue,
        )
        .expect_err("oversized output must fail");
        assert!(error.to_string().contains("output exceeds"));
    }

    #[test]
    fn fuel_and_control_interrupt_infinite_guest() {
        let manifest = manifest();
        let infinite = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "jarvis_alloc") (param i32) (result i32) i32.const 0)
                (func (export "jarvis_run") (param i32 i32) (result i64)
                    (loop $forever (br $forever))
                    i64.const 0))"#,
        )
        .unwrap();
        let mut checks = 0;
        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &infinite,
            &json!({}),
            || {
                checks += 1;
                if checks >= 3 {
                    WasmControlState::EmergencyPaused
                } else {
                    WasmControlState::Continue
                }
            },
        )
        .expect_err("pause must interrupt guest");
        assert!(error.to_string().contains("emergency pause"));

        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &infinite,
            &json!({}),
            || WasmControlState::Continue,
        )
        .expect_err("fuel must bound guest");
        assert!(error.to_string().contains("fuel limit"));
    }

    #[test]
    fn invalid_output_schema_and_cancelled_before_compile_are_blocked() {
        let manifest = manifest();
        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &module_returning(r#"{"unexpected":true}"#),
            &json!({}),
            || WasmControlState::Continue,
        )
        .expect_err("schema mismatch must fail");
        assert!(error.to_string().contains("output"));

        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &module_returning(r#"{"ok":true}"#),
            &json!({}),
            || WasmControlState::Cancelled,
        )
        .expect_err("cancel must win before compile");
        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn completed_call_cannot_publish_after_deadline() {
        let mut manifest = manifest();
        manifest.actions[0].timeout.timeout_ms = 1;
        let mut checks = 0;
        let error = execute_installed_wasm_plugin(
            &manifest,
            &manifest.actions[0],
            &module_returning(r#"{"ok":true}"#),
            &json!({}),
            || {
                checks += 1;
                if checks == 2 {
                    std::thread::sleep(Duration::from_millis(5));
                }
                WasmControlState::Continue
            },
        )
        .expect_err("deadline must dominate a completed call");
        assert!(error.to_string().contains("timed out"));
    }

    #[cfg(unix)]
    #[test]
    fn artifact_reader_rejects_symlink_and_oversize() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let module = dir.path().join("module.wasm");
        std::fs::write(&module, b"wasm").unwrap();
        let link = dir.path().join("link.wasm");
        symlink(&module, &link).unwrap();
        assert!(read_wasm_artifact(&link).is_err());

        let oversized = dir.path().join("oversized.wasm");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len((MAX_WASM_MODULE_BYTES + 1) as u64).unwrap();
        assert!(read_wasm_artifact(&oversized).is_err());
    }
}
