use assemblywright_protocol::{
    AttemptId, CancellationId, CapabilityDescriptor, CapabilityKind, ContextHandlingPolicy,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId,
    TaskId, MAX_JOB_CONTEXT_BYTES, MAX_JOB_RESULT_BYTES, MAX_MLX_PROMPT_BYTES,
    MLX_REASONING_CAPABILITY_ID, MLX_REASONING_PROVIDER, PROTOCOL_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn mlx_job(prompt: &str) -> JobEnvelope {
    let context = json!({
        "operation": "generate_text",
        "prompt": prompt,
        "max_tokens": 128,
        "temperature_milli": 700
    });
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 9,
        sequence: 1,
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        attempt_id: AttemptId::new(Uuid::new_v4()),
        lease_id: LeaseId::new(Uuid::new_v4()),
        cancellation_id: CancellationId::new(Uuid::new_v4()),
        capability_id: MLX_REASONING_CAPABILITY_ID.to_string(),
        selected_model: "mlx-community/Qwen3-4B-4bit".to_string(),
        sensitivity: Sensitivity::Public,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
        context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
        context,
    }
}

fn mlx_result(job: &JobEnvelope, output: &str) -> JobResultEnvelope {
    let payload = json!({
        "operation": "generate_text",
        "output": output,
        "model": job.selected_model
    });
    JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: job.sequence + 1,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: Sha256::digest(serde_json::to_vec(&payload).unwrap()).into(),
        payload,
    }
}

#[test]
fn mlx_capability_is_exact_local_inference_with_configured_bounds() {
    let capability = CapabilityDescriptor::mlx_reasoning(
        "mlx-community/Qwen3-4B-4bit",
        MAX_JOB_CONTEXT_BYTES as u32,
        MAX_JOB_RESULT_BYTES as u32,
    );
    capability.validate().expect("exact MLX capability");
    assert_eq!(capability.id, MLX_REASONING_CAPABILITY_ID);
    assert_eq!(capability.kind, CapabilityKind::LocalInference);
    assert_eq!(capability.provider, MLX_REASONING_PROVIDER);

    let mut wrong_provider = capability.clone();
    wrong_provider.provider = "other".to_string();
    assert_eq!(
        wrong_provider.validate(),
        Err(ProtocolError::InvalidMlxCapability)
    );
    let mut empty_model = capability.clone();
    empty_model.model.clear();
    assert!(empty_model.validate().is_err());
}

#[test]
fn mlx_job_accepts_only_exact_public_ephemeral_generate_text_contract() {
    mlx_job("bounded prompt")
        .validate_mlx_reasoning()
        .expect("exact MLX job");

    let mut unknown = mlx_job("prompt");
    unknown.context["extra"] = json!(true);
    unknown.context_sha256 = Sha256::digest(serde_json::to_vec(&unknown.context).unwrap()).into();
    assert_eq!(
        unknown.validate_mlx_reasoning(),
        Err(ProtocolError::InvalidMlxJob)
    );

    for context in [
        json!({"operation":"generate_text","prompt":"","max_tokens":1,"temperature_milli":0}),
        json!({"operation":"other","prompt":"p","max_tokens":1,"temperature_milli":0}),
        json!({"operation":"generate_text","prompt":"p","max_tokens":0,"temperature_milli":0}),
        json!({"operation":"generate_text","prompt":"p","max_tokens":513,"temperature_milli":0}),
        json!({"operation":"generate_text","prompt":"p","max_tokens":1,"temperature_milli":2001}),
    ] {
        let mut job = mlx_job("prompt");
        job.context = context;
        job.context_sha256 = Sha256::digest(serde_json::to_vec(&job.context).unwrap()).into();
        assert_eq!(
            job.validate_mlx_reasoning(),
            Err(ProtocolError::InvalidMlxJob)
        );
    }

    let oversized = "x".repeat(MAX_MLX_PROMPT_BYTES + 1);
    assert_eq!(
        mlx_job(&oversized).validate_mlx_reasoning(),
        Err(ProtocolError::InvalidMlxJob)
    );
    let mut non_public = mlx_job("prompt");
    non_public.sensitivity = Sensitivity::Workspace;
    assert_eq!(
        non_public.validate_mlx_reasoning(),
        Err(ProtocolError::InvalidMlxJob)
    );
}

#[test]
fn mlx_result_binds_exact_model_and_nonempty_bounded_output() {
    let job = mlx_job("prompt");
    mlx_result(&job, "bounded output")
        .validate_mlx_reasoning_result(&job)
        .expect("exact MLX result");

    let mut wrong_model = mlx_result(&job, "output");
    wrong_model.payload["model"] = json!("different-model");
    wrong_model.payload_sha256 =
        Sha256::digest(serde_json::to_vec(&wrong_model.payload).unwrap()).into();
    assert_eq!(
        wrong_model.validate_mlx_reasoning_result(&job),
        Err(ProtocolError::InvalidMlxResult)
    );
    assert_eq!(
        mlx_result(&job, "").validate_mlx_reasoning_result(&job),
        Err(ProtocolError::InvalidMlxResult)
    );
    let mut unknown = mlx_result(&job, "output");
    unknown.payload["extra"] = json!(true);
    unknown.payload_sha256 = Sha256::digest(serde_json::to_vec(&unknown.payload).unwrap()).into();
    assert_eq!(
        unknown.validate_mlx_reasoning_result(&job),
        Err(ProtocolError::InvalidMlxResult)
    );
}
