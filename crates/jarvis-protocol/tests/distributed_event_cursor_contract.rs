use jarvis_protocol::{
    DeviceId, DistributedEvent, DistributedEventBatch, DistributedEventBatchRequest,
    DistributedEventCursor, DistributedEventKind, ProtocolError, StepId, TaskId,
    MAX_DISTRIBUTED_EVENTS_PER_BATCH, PROTOCOL_VERSION,
};
use uuid::Uuid;

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed UUID")
}

fn queued_event(stream_id: Uuid, sequence: u64) -> DistributedEvent {
    DistributedEvent {
        protocol_version: PROTOCOL_VERSION,
        cursor: DistributedEventCursor {
            stream_id,
            sequence,
        },
        occurred_at_ms: 1_000 + sequence,
        kind: DistributedEventKind::StepQueued,
        task_id: Some(TaskId::new(uuid("22222222-2222-4222-8222-222222222222"))),
        step_id: Some(StepId::new(uuid("33333333-3333-4333-8333-333333333333"))),
        device_id: None,
        connection_epoch: None,
    }
}

#[test]
fn distributed_event_batch_requires_one_contiguous_server_stream() {
    let stream_id = uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let batch = DistributedEventBatch {
        protocol_version: PROTOCOL_VERSION,
        stream_id,
        after_sequence: 40,
        next_sequence: 42,
        events: vec![
            queued_event(stream_id, 41),
            DistributedEvent {
                protocol_version: PROTOCOL_VERSION,
                cursor: DistributedEventCursor {
                    stream_id,
                    sequence: 42,
                },
                occurred_at_ms: 1_042,
                kind: DistributedEventKind::DeviceConnected,
                task_id: None,
                step_id: None,
                device_id: Some(DeviceId::new(uuid("11111111-1111-4111-8111-111111111111"))),
                connection_epoch: Some(9),
            },
        ],
        has_more: false,
    };
    batch.validate().expect("valid contiguous event batch");
    DistributedEventBatch::decode_frame(&serde_json::to_vec(&batch).expect("encode event batch"))
        .expect("strict event batch frame");

    let mut gap = batch.clone();
    gap.events[1].cursor.sequence = 43;
    assert_eq!(gap.validate(), Err(ProtocolError::EventCursorGap));

    let mut other_stream = batch.clone();
    other_stream.events[0].cursor.stream_id = uuid("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    assert_eq!(other_stream.validate(), Err(ProtocolError::EventCursorGap));
}

#[test]
fn distributed_event_requests_and_identities_fail_closed() {
    let stream_id = uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    let request = DistributedEventBatchRequest {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 9,
        after: Some(DistributedEventCursor {
            stream_id,
            sequence: 41,
        }),
        limit: MAX_DISTRIBUTED_EVENTS_PER_BATCH as u16,
    };
    request.validate().expect("bounded cursor request");

    let mut zero_limit = request;
    zero_limit.limit = 0;
    assert!(matches!(
        zero_limit.validate(),
        Err(ProtocolError::InvalidLimit { field: "limit", .. })
    ));

    let mut wrong_identity = queued_event(stream_id, 1);
    wrong_identity.device_id = Some(DeviceId::new(uuid("11111111-1111-4111-8111-111111111111")));
    assert_eq!(
        wrong_identity.validate(),
        Err(ProtocolError::DistributedEventIdentityMismatch)
    );

    let mut unknown = serde_json::to_value(DistributedEventBatch {
        protocol_version: PROTOCOL_VERSION,
        stream_id,
        after_sequence: 0,
        next_sequence: 1,
        events: vec![queued_event(stream_id, 1)],
        has_more: false,
    })
    .expect("encode batch");
    unknown["events"][0]["payload"] = serde_json::json!({"secret":"not allowed"});
    assert!(matches!(
        DistributedEventBatch::decode_frame(&serde_json::to_vec(&unknown).unwrap()),
        Err(ProtocolError::Deserialization { .. })
    ));
}
