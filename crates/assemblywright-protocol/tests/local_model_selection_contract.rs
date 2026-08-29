use assemblywright_protocol::{
    DeviceId, LocalModelSelectionRequest, LOCAL_MODEL_SELECTION_SCHEMA_VERSION,
};
use uuid::Uuid;

fn request() -> LocalModelSelectionRequest {
    LocalModelSelectionRequest {
        schema_version: LOCAL_MODEL_SELECTION_SCHEMA_VERSION,
        device_id: DeviceId::new(Uuid::new_v4()),
        expected_registry_revision: 3,
        expected_designation_revision: 7,
        expected_emergency_pause_revision: 2,
        model_id: "mlx-community/Qwen3-8B-4bit".to_string(),
    }
}

#[test]
fn local_model_selection_request_is_strict_bounded_and_path_free() {
    let expected = request();
    let valid = serde_json::to_vec(&expected).unwrap();
    assert_eq!(
        LocalModelSelectionRequest::decode_frame(&valid).unwrap(),
        expected
    );

    let mut unknown: serde_json::Value = serde_json::from_slice(&valid).unwrap();
    unknown["model_directory"] = serde_json::json!("/Users/owner/models/qwen");
    assert!(
        LocalModelSelectionRequest::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err()
    );

    let duplicate = format!(
        "{{\"schema_version\":1,\"device_id\":\"{}\",\"expected_registry_revision\":3,\"expected_designation_revision\":7,\"expected_emergency_pause_revision\":2,\"model_id\":\"one\",\"model_id\":\"two\"}}",
        request().device_id.0
    );
    assert!(LocalModelSelectionRequest::decode_frame(duplicate.as_bytes()).is_err());
}

#[test]
fn local_model_selection_rejects_empty_stale_shape_and_path_like_model() {
    let mut invalid = request();
    invalid.expected_registry_revision = 0;
    assert!(invalid.validate().is_err());
    invalid = request();
    invalid.expected_designation_revision = 0;
    assert!(invalid.validate().is_err());
    invalid = request();
    invalid.model_id.clear();
    assert!(invalid.validate().is_err());
    invalid = request();
    invalid.model_id = "/Users/owner/models/qwen".to_string();
    // Model IDs may contain one namespace slash, but absolute local paths are forbidden.
    assert!(invalid.model_id.starts_with('/'));
    assert!(invalid.validate().is_err());

    for model_id in [
        "model with space",
        "model\twith-tab",
        "model\nwith-newline",
        "mlx-community/Qwen-é",
    ] {
        invalid = request();
        invalid.model_id = model_id.to_string();
        assert!(
            invalid.validate().is_err(),
            "non-printable-ASCII model ID was accepted: {model_id:?}"
        );
    }
}
