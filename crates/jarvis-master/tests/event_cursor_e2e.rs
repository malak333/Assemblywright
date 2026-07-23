use jarvis_master::{DeviceRegistration, MasterError, MasterKernel, NewStep};
use jarvis_protocol::{
    CapabilityDescriptor, CapabilityKind, DeviceId, DeviceRole, DistributedEventBatchRequest,
    DistributedEventCursor, DistributedEventKind, HandshakeRequest, Sensitivity, StepId, TaskId,
    PROTOCOL_VERSION,
};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed UUID")
}

fn registration() -> DeviceRegistration {
    DeviceRegistration {
        device_id: DeviceId::new(uuid("11111111-1111-4111-8111-111111111111")),
        device_name: "owner-mac-agent".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor {
            id: "m1.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "fake-mlx".to_string(),
            model: "fake-local".to_string(),
            max_context_bytes: 262_144,
            max_result_bytes: 786_432,
        }],
    }
}

fn request(
    connection_epoch: u64,
    after: Option<DistributedEventCursor>,
    limit: u16,
) -> DistributedEventBatchRequest {
    DistributedEventBatchRequest {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch,
        after,
        limit,
    }
}

#[test]
fn master_event_cursor_is_durable_contiguous_and_stream_bound() {
    let directory = tempdir().expect("temporary master directory");
    let database = directory.path().join("master.sqlite3");
    let registration = registration();
    let handshake = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: registration.device_id,
        device_name: registration.device_name.clone(),
        role: registration.role,
        registry_revision: registration.registry_revision,
        capabilities: registration.capabilities.clone(),
    };
    let mut master = MasterKernel::open(&database).expect("open master");
    master
        .register_device(&registration)
        .expect("register Mac agent");
    let accepted = master
        .accept_handshake(&handshake, 1_000)
        .expect("accept Mac agent");
    master
        .enqueue_step(
            &NewStep {
                task_id: TaskId::new(uuid("22222222-2222-4222-8222-222222222222")),
                step_id: StepId::new(uuid("33333333-3333-4333-8333-333333333333")),
                capability_id: "m1.reasoning".to_string(),
                sensitivity: Sensitivity::Workspace,
                context: json!({"prompt":"metadata must not enter the event stream"}),
                lease_duration_ms: 60_000,
                deadline_after_ms: 300_000,
            },
            2_000,
        )
        .expect("enqueue step");

    let first = master
        .distributed_events(&request(accepted.connection_epoch, None, 1))
        .expect("read first bounded page");
    assert_eq!(first.events.len(), 1);
    assert!(first.has_more);
    assert_eq!(first.events[0].kind, DistributedEventKind::DeviceConnected);
    assert_eq!(first.events[0].cursor.sequence, 1);
    assert_eq!(first.next_sequence, 1);

    let second = master
        .distributed_events(&request(
            accepted.connection_epoch,
            Some(first.events[0].cursor),
            64,
        ))
        .expect("resume after exact cursor");
    assert_eq!(second.events.len(), 1);
    assert!(!second.has_more);
    assert_eq!(second.events[0].kind, DistributedEventKind::StepQueued);
    assert_eq!(second.events[0].cursor.sequence, 2);
    let encoded = serde_json::to_string(&second).expect("encode redacted event batch");
    assert!(!encoded.contains("metadata must not enter"));
    master
        .lease_next_step(registration.device_id, accepted.connection_epoch, 3_000)
        .expect("lease queued step");
    let leased = master
        .distributed_events(&request(
            accepted.connection_epoch,
            Some(second.events[0].cursor),
            64,
        ))
        .expect("read lease transition");
    assert_eq!(leased.events.len(), 1);
    assert_eq!(leased.events[0].kind, DistributedEventKind::StepLeased);
    drop(master);

    let reopened = MasterKernel::open(&database).expect("reopen durable master");
    let resumed = reopened
        .distributed_events(&request(
            accepted.connection_epoch,
            Some(leased.events[0].cursor),
            64,
        ))
        .expect("resume exact durable high-water after restart");
    assert_eq!(resumed.stream_id, leased.stream_id);
    assert_eq!(resumed.events.len(), 2);
    assert_eq!(
        resumed.events[0].kind,
        DistributedEventKind::DeviceDisconnected
    );
    assert_eq!(resumed.events[1].kind, DistributedEventKind::StepQueued);
    assert_eq!(resumed.events[1].cursor.sequence, leased.next_sequence + 2);

    let wrong_stream = DistributedEventCursor {
        stream_id: uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
        sequence: resumed.next_sequence,
    };
    assert!(matches!(
        reopened.distributed_events(&request(accepted.connection_epoch, Some(wrong_stream), 64)),
        Err(MasterError::EventCursorStreamMismatch)
    ));

    let ahead = DistributedEventCursor {
        stream_id: resumed.stream_id,
        sequence: resumed.next_sequence + 1,
    };
    assert!(matches!(
        reopened.distributed_events(&request(accepted.connection_epoch, Some(ahead), 64)),
        Err(MasterError::EventCursorAhead)
    ));
}
