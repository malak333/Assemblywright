use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

const MAX_IDS: usize = 129;

pub(super) fn tag_message_id(uuid: Uuid) -> String {
    format!("msg_{}", uuid)
}

fn validate_tagged_id(value: &str, prefix: &str) -> Result<()> {
    if !value.starts_with(prefix) {
        bail!("invalid prefix");
    }
    let suffix = &value[prefix.len()..];
    if suffix.is_empty() {
        bail!("empty suffix");
    }
    let bytes = suffix.as_bytes();
    if bytes.len() > 196 {
        bail!("suffix too long");
    }
    for &b in bytes {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            bail!("invalid suffix character");
        }
    }
    Ok(())
}

pub(super) fn validate_session_id(value: &str) -> Result<()> {
    validate_tagged_id(value, "ses_")
}

pub(super) fn validate_message_id(value: &str) -> Result<()> {
    validate_tagged_id(value, "msg_")
}

pub(super) struct ResponseHintTracker {
    candidate_message_ids: BTreeSet<String>,
    tool_message_ids: BTreeSet<String>,
    expected_session_id: String,
    submitted_user_message_id: String,
}

impl ResponseHintTracker {
    pub(super) fn new(expected_session_id: &str, submitted_user_message_id: &str) -> Result<Self> {
        validate_session_id(expected_session_id)?;
        validate_message_id(submitted_user_message_id)?;
        Ok(Self {
            candidate_message_ids: BTreeSet::new(),
            tool_message_ids: BTreeSet::new(),
            expected_session_id: expected_session_id.to_string(),
            submitted_user_message_id: submitted_user_message_id.to_string(),
        })
    }

    pub(super) fn observe_message_updated(&mut self, info: &Value) -> Result<()> {
        let role = match info.get("role").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return Ok(()),
        };
        if role != "assistant" {
            return Ok(());
        }

        let parent = match info.get("parentID").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return Ok(()),
        };
        if parent != self.submitted_user_message_id {
            return Ok(());
        }

        let session_id = match info.get("sessionID").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => bail!("missing sessionID"),
        };
        validate_session_id(session_id)?;

        let message_id = match info.get("id").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => bail!("missing message id"),
        };
        validate_message_id(message_id)?;

        if session_id != self.expected_session_id {
            return Ok(());
        }

        if info.get("error").is_some() {
            bail!("error present");
        }

        let finish = match info.get("finish").and_then(|v| v.as_str()) {
            Some(f) => f,
            None => return Ok(()),
        };
        if finish != "stop" {
            return Ok(());
        }

        if self.tool_message_ids.contains(message_id) {
            return Ok(());
        }

        if self.candidate_message_ids.contains(message_id) {
            return Ok(());
        }

        if self.candidate_message_ids.len() >= MAX_IDS {
            bail!("candidate set full");
        }

        self.candidate_message_ids.insert(message_id.to_string());
        Ok(())
    }

    pub(super) fn observe_part_updated(&mut self, part: &Value) -> Result<()> {
        let part_type = match part.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return Ok(()),
        };
        if part_type != "tool" {
            return Ok(());
        }

        let session_id = match part.get("sessionID").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => bail!("missing sessionID in part"),
        };
        validate_session_id(session_id)?;

        if session_id != self.expected_session_id {
            return Ok(());
        }

        let message_id = match part.get("messageID").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => bail!("missing messageID in part"),
        };
        validate_message_id(message_id)?;

        if !self.tool_message_ids.contains(message_id) && self.tool_message_ids.len() >= MAX_IDS {
            bail!("tool set full");
        }

        if self.candidate_message_ids.contains(message_id) {
            self.candidate_message_ids.remove(message_id);
        }

        self.tool_message_ids.insert(message_id.to_string());
        Ok(())
    }

    pub(super) fn unique_candidate(&self) -> Result<String> {
        if self.candidate_message_ids.len() == 1 {
            Ok(self.candidate_message_ids.iter().next().unwrap().clone())
        } else {
            bail!("not exactly one candidate")
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum BoundAssistantResponse {
    Text(String),
    SensitiveWithheld,
}

pub(super) fn parse_targeted_message(
    bytes: &[u8],
    expected_session_id: &str,
    submitted_user_message_id: &str,
    expected_assistant_message_id: &str,
    sensitive_read_seen: bool,
) -> Result<BoundAssistantResponse> {
    validate_session_id(expected_session_id)?;
    validate_message_id(submitted_user_message_id)?;
    validate_message_id(expected_assistant_message_id)?;

    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("invalid json"))?;

    let root = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("root not object"))?;
    let info = root
        .get("info")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing info"))?;
    let parts = root
        .get("parts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing parts"))?;

    if info.contains_key("error") {
        return Err(anyhow::anyhow!("info contains error"));
    }

    let info_id = info
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info.id"))?;
    let info_session = info
        .get("sessionID")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info.sessionID"))?;
    let info_parent = info
        .get("parentID")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info.parentID"))?;
    let info_role = info
        .get("role")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info.role"))?;
    let info_finish = info
        .get("finish")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info.finish"))?;

    if info_id != expected_assistant_message_id {
        return Err(anyhow::anyhow!("info.id mismatch"));
    }
    if info_session != expected_session_id {
        return Err(anyhow::anyhow!("info.sessionID mismatch"));
    }
    if info_parent != submitted_user_message_id {
        return Err(anyhow::anyhow!("info.parentID mismatch"));
    }
    if info_role != "assistant" {
        return Err(anyhow::anyhow!("info.role mismatch"));
    }
    if info_finish != "stop" {
        return Err(anyhow::anyhow!("info.finish mismatch"));
    }

    if parts.len() > 256 {
        return Err(anyhow::anyhow!("too many parts"));
    }

    let mut text_parts_count: usize = 0;
    let mut total_text_bytes: usize = 0;
    let mut has_non_whitespace = false;

    for part in parts {
        let part_obj = part
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("part not object"))?;

        let part_id = part_obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing part id"))?;
        if part_id.is_empty() || part_id.len() > 200 {
            return Err(anyhow::anyhow!("invalid part id"));
        }

        let part_session = part_obj
            .get("sessionID")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing part sessionID"))?;
        validate_session_id(part_session)?;
        if part_session != expected_session_id {
            return Err(anyhow::anyhow!("part sessionID mismatch"));
        }

        let part_message_id = part_obj
            .get("messageID")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing part messageID"))?;
        validate_message_id(part_message_id)?;
        if part_message_id != expected_assistant_message_id {
            return Err(anyhow::anyhow!("part messageID mismatch"));
        }

        let part_type = part_obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing part type"))?;
        if part_type == "tool" {
            return Err(anyhow::anyhow!("tool part rejected"));
        }

        if part_type == "text" {
            let text = part_obj
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing text"))?;
            let text_bytes = text.len();
            total_text_bytes = total_text_bytes
                .checked_add(text_bytes)
                .ok_or_else(|| anyhow::anyhow!("text overflow"))?;
            if total_text_bytes > 64000 {
                return Err(anyhow::anyhow!("text too large"));
            }
            text_parts_count = text_parts_count
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("text count overflow"))?;
            if text_parts_count > 256 {
                return Err(anyhow::anyhow!("too many text parts"));
            }
            if !text.trim().is_empty() {
                has_non_whitespace = true;
            }
        }
    }

    if text_parts_count == 0 {
        return Err(anyhow::anyhow!("no text parts"));
    }
    if !has_non_whitespace {
        return Err(anyhow::anyhow!("no non-whitespace text"));
    }

    if sensitive_read_seen {
        return Ok(BoundAssistantResponse::SensitiveWithheld);
    }

    let mut result = String::new();
    for part in parts {
        let part_obj = part
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("part not object"))?;
        let part_type = part_obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing part type"))?;
        if part_type == "text" {
            let text = part_obj
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing text"))?;
            result.push_str(text);
        }
    }
    Ok(BoundAssistantResponse::Text(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tag_message_id() {
        let uuid = Uuid::nil();
        assert_eq!(
            tag_message_id(uuid),
            "msg_00000000-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn test_validate_session_id_valid() {
        assert!(validate_session_id("ses_abc").is_ok());
        assert!(validate_session_id("ses_123").is_ok());
        assert!(validate_session_id("ses_a-b_c").is_ok());
    }

    #[test]
    fn test_validate_session_id_invalid_prefix() {
        assert!(validate_session_id("msg_abc").is_err());
        assert!(validate_session_id("abc").is_err());
    }

    #[test]
    fn test_validate_session_id_empty_suffix() {
        assert!(validate_session_id("ses_").is_err());
    }

    #[test]
    fn test_validate_session_id_invalid_chars() {
        assert!(validate_session_id("ses_a/b").is_err());
        assert!(validate_session_id("ses_a.b").is_err());
        assert!(validate_session_id("ses_a%b").is_err());
        assert!(validate_session_id("ses_a\x00b").is_err());
        assert!(validate_session_id("ses_aéb").is_err());
    }

    #[test]
    fn test_validate_session_id_max_length() {
        let suffix_196 = "a".repeat(196);
        let id_200 = format!("ses_{}", suffix_196);
        assert!(validate_session_id(&id_200).is_ok());

        let suffix_197 = "a".repeat(197);
        let id_201 = format!("ses_{}", suffix_197);
        assert!(validate_session_id(&id_201).is_err());
    }

    #[test]
    fn test_validate_message_id_valid() {
        assert!(validate_message_id("msg_abc").is_ok());
    }

    #[test]
    fn test_validate_message_id_invalid_prefix() {
        assert!(validate_message_id("ses_abc").is_err());
    }

    #[test]
    fn test_validate_message_id_empty_suffix() {
        assert!(validate_message_id("msg_").is_err());
    }

    #[test]
    fn test_validate_message_id_invalid_chars() {
        assert!(validate_message_id("msg_a/b").is_err());
    }

    #[test]
    fn test_validate_message_id_max_length() {
        let suffix_196 = "a".repeat(196);
        let id_200 = format!("msg_{}", suffix_196);
        assert!(validate_message_id(&id_200).is_ok());

        let suffix_197 = "a".repeat(197);
        let id_201 = format!("msg_{}", suffix_197);
        assert!(validate_message_id(&id_201).is_err());
    }

    #[test]
    fn test_tracker_new_valid() {
        let tracker = ResponseHintTracker::new("ses_1", "msg_1");
        assert!(tracker.is_ok());
    }

    #[test]
    fn test_tracker_new_invalid_session() {
        let tracker = ResponseHintTracker::new("msg_1", "msg_1");
        assert!(tracker.is_err());
    }

    #[test]
    fn test_tracker_new_invalid_message() {
        let tracker = ResponseHintTracker::new("ses_1", "ses_1");
        assert!(tracker.is_err());
    }

    #[test]
    fn test_observe_message_wrong_role() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "user", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_message_wrong_parent() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_2", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_message_missing_session() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info =
            json!({ "role": "assistant", "parentID": "msg_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());
    }

    #[test]
    fn test_observe_message_missing_id() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());
    }

    #[test]
    fn test_observe_message_valid_other_session() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_2", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_message_error_present() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "error": null, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());
    }

    #[test]
    fn test_observe_message_error_present_string() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "error": "oops", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());
    }

    #[test]
    fn test_observe_message_non_stop_finish() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "timeout" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_message_missing_finish() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_message_exact_stop_admitted() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert_eq!(tracker.unique_candidate().unwrap(), "msg_2");
    }

    #[test]
    fn test_observe_message_duplicate_candidate() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert_eq!(tracker.unique_candidate().unwrap(), "msg_2");
    }

    #[test]
    fn test_observe_message_tool_conflict() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        let part =
            json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_2", "id": "part_1" });
        assert!(tracker.observe_part_updated(&part).is_ok());

        assert!(tracker.unique_candidate().is_err());
        assert!(tracker.tool_message_ids.contains("msg_2"));
    }

    #[test]
    fn test_observe_part_wrong_type() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "text", "sessionID": "ses_1", "messageID": "msg_2" });
        assert!(tracker.observe_part_updated(&part).is_ok());
    }

    #[test]
    fn test_observe_part_missing_session() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "messageID": "msg_2" });
        assert!(tracker.observe_part_updated(&part).is_err());
    }

    #[test]
    fn test_observe_part_wrong_session() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "sessionID": "ses_2", "messageID": "msg_2" });
        assert!(tracker.observe_part_updated(&part).is_ok());
    }

    #[test]
    fn test_observe_part_missing_message_id() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "sessionID": "ses_1" });
        assert!(tracker.observe_part_updated(&part).is_err());
    }

    #[test]
    fn test_observe_part_invalid_message_id() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "ses_2" });
        assert!(tracker.observe_part_updated(&part).is_err());
    }

    #[test]
    fn test_observe_part_unrelated_tool() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_99" });
        assert!(tracker.observe_part_updated(&part).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_part_matching_tool_removes_candidate() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_2" });
        assert!(tracker.observe_part_updated(&part).is_ok());

        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_part_conflicting_part_id() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        let part =
            json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_2", "id": "part_99" });
        assert!(tracker.observe_part_updated(&part).is_ok());

        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_part_tool_before_candidate() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_2" });
        assert!(tracker.observe_part_updated(&part).is_ok());

        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_observe_part_unrelated_tool_does_not_remove_candidate() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_99" });
        assert!(tracker.observe_part_updated(&part).is_ok());

        assert_eq!(tracker.unique_candidate().unwrap(), "msg_2");
    }

    #[test]
    fn test_128_unrelated_tools_then_candidate() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        for i in 0..128 {
            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": format!("msg_tool_{}", i) });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }

        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_candidate", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        assert_eq!(tracker.unique_candidate().unwrap(), "msg_candidate");
    }

    #[test]
    fn test_129_unique_candidates_accepted() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        for i in 0..129 {
            let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": format!("msg_{}", i), "finish": "stop" });
            assert!(tracker.observe_message_updated(&info).is_ok());
        }
        assert_eq!(tracker.candidate_message_ids.len(), 129);
    }

    #[test]
    fn test_130th_candidate_rejected() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        for i in 0..129 {
            let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": format!("msg_{}", i), "finish": "stop" });
            assert!(tracker.observe_message_updated(&info).is_ok());
        }
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_129", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());
    }

    #[test]
    fn test_129_unique_tools_accepted() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        for i in 0..129 {
            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": format!("msg_tool_{}", i) });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }
        assert_eq!(tracker.tool_message_ids.len(), 129);
    }

    #[test]
    fn test_130th_tool_rejected() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        for i in 0..129 {
            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": format!("msg_tool_{}", i) });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_tool_129" });
        assert!(tracker.observe_part_updated(&part).is_err());
    }

    #[test]
    fn test_zero_candidates_reject() {
        let tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_two_candidates_reject() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info1 = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "finish": "stop" });
        let info2 = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_2", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info1).is_ok());
        assert!(tracker.observe_message_updated(&info2).is_ok());
        assert!(tracker.unique_candidate().is_err());
    }

    #[test]
    fn test_duplicate_candidates_free() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "finish": "stop" });
        for _ in 0..130 {
            assert!(tracker.observe_message_updated(&info).is_ok());
        }
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), "msg_1");
    }

    #[test]
    fn test_malformed_update_after_valid_candidate_returns_error() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());

        let info_bad = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "error": "oops", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info_bad).is_err());
    }
    #[test]
    fn test_capacity_idempotence() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let mut candidate_ids = Vec::new();
        let mut tool_ids = Vec::new();

        for i in 0..129 {
            let cid = format!("msg_cand_{}", i);
            let tid = format!("msg_tool_{}", i);
            candidate_ids.push(cid.clone());
            tool_ids.push(tid.clone());

            let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": cid, "finish": "stop" });
            assert!(tracker.observe_message_updated(&info).is_ok());

            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": tid });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }

        assert_eq!(tracker.candidate_message_ids.len(), 129);
        assert_eq!(tracker.tool_message_ids.len(), 129);

        // Repeat one existing ID in each; both calls succeed and each length remains 129
        let info_dup = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": candidate_ids[0], "finish": "stop" });
        assert!(tracker.observe_message_updated(&info_dup).is_ok());
        assert_eq!(tracker.candidate_message_ids.len(), 129);

        let part_dup = json!({ "type": "tool", "sessionID": "ses_1", "messageID": tool_ids[0] });
        assert!(tracker.observe_part_updated(&part_dup).is_ok());
        assert_eq!(tracker.tool_message_ids.len(), 129);
    }

    #[test]
    fn test_novel_130th_tool_fails_and_candidate_remains() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let candidate_id = "msg_cand_1";
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": candidate_id, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert_eq!(tracker.candidate_message_ids.len(), 1);

        // Fill tool set to 129 with unrelated IDs
        for i in 0..129 {
            let tid = format!("msg_tool_{}", i);
            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": tid });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }
        assert_eq!(tracker.tool_message_ids.len(), 129);

        // Observe a novel 130th tool whose messageID equals that candidate
        let novel_tool_id = candidate_id;
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": novel_tool_id });
        assert!(tracker.observe_part_updated(&part).is_err());

        // The candidate remains the unique candidate
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);
    }

    #[test]
    fn test_malformed_field_types_and_invalid_tagged_ids() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();

        // Malformed sessionID (non-string)
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": 123, "id": "msg_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());

        // Malformed messageID (non-string)
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": 123, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());

        // Invalid tagged ID (wrong prefix)
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "bad_id", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_err());

        // Tool part with malformed sessionID
        let part = json!({ "type": "tool", "sessionID": 123, "messageID": "msg_1" });
        assert!(tracker.observe_part_updated(&part).is_err());

        // Tool part with malformed messageID
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": 123 });
        assert!(tracker.observe_part_updated(&part).is_err());
    }

    #[test]
    fn test_ignored_malformed_role_parent_finish() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();

        // Non-string role
        let info = json!({ "role": 123, "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.candidate_message_ids.is_empty());

        // Non-string parent
        let info = json!({ "role": "assistant", "parentID": 123, "sessionID": "ses_1", "id": "msg_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.candidate_message_ids.is_empty());

        // Non-string finish
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "finish": 123 });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert!(tracker.candidate_message_ids.is_empty());
    }

    #[test]
    fn test_tool_part_id_ignores_candidate_matching() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let candidate_id = "msg_cand_1";
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": candidate_id, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert_eq!(tracker.candidate_message_ids.len(), 1);

        // A tool part whose own `id` equals the candidate but whose `messageID` is unrelated
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": "msg_unrelated", "id": candidate_id });
        assert!(tracker.observe_part_updated(&part).is_ok());

        // The candidate remains intact because messageID did not match
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);
    }

    #[test]
    fn test_duplicate_tool_observations_idempotent() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let tool_id = "msg_tool_1";

        // Fill tool set to 129
        for i in 0..129 {
            let tid = format!("msg_tool_{}", i);
            let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": tid });
            assert!(tracker.observe_part_updated(&part).is_ok());
        }
        assert_eq!(tracker.tool_message_ids.len(), 129);

        // Duplicate observation at capacity
        let part = json!({ "type": "tool", "sessionID": "ses_1", "messageID": tool_id });
        assert!(tracker.observe_part_updated(&part).is_ok());
        assert_eq!(tracker.tool_message_ids.len(), 129);
    }

    #[test]
    fn test_sensitive_marker_in_error() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": "msg_1", "error": "SENSITIVE_MARKER_482", "finish": "stop" });
        let res = tracker.observe_message_updated(&info);
        assert!(res.is_err());
        let err_msg = format!("{}", res.unwrap_err());
        assert!(!err_msg.contains("SENSITIVE_MARKER_482"));
    }

    #[test]
    fn test_malformed_updates_after_admission_preserve_unique_hint() {
        let mut tracker = ResponseHintTracker::new("ses_1", "msg_1").unwrap();

        // 1. Admit one exact valid stop candidate
        let candidate_id = "msg_cand_1";
        let info = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": candidate_id, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info).is_ok());
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);

        // 2. Submit relevant assistant update with non-string sessionID and assert Err
        let info_bad_session = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": 123, "id": "msg_update_1", "finish": "stop" });
        assert!(tracker.observe_message_updated(&info_bad_session).is_err());
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);

        // 3. Submit relevant assistant update with expected session but non-string id and assert Err
        let info_bad_id = json!({ "role": "assistant", "parentID": "msg_1", "sessionID": "ses_1", "id": 456, "finish": "stop" });
        assert!(tracker.observe_message_updated(&info_bad_id).is_err());
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);

        // 4. Submit tool part with non-string sessionID and assert Err
        let part_bad_session =
            json!({ "type": "tool", "sessionID": 789, "messageID": "msg_tool_1" });
        assert!(tracker.observe_part_updated(&part_bad_session).is_err());
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);

        // 5. Submit tool part for expected session with non-string messageID and assert Err
        let part_bad_msgid = json!({ "type": "tool", "sessionID": "ses_1", "messageID": 999 });
        assert!(tracker.observe_part_updated(&part_bad_msgid).is_err());
        assert_eq!(tracker.candidate_message_ids.len(), 1);
        assert_eq!(tracker.unique_candidate().unwrap(), candidate_id);
    }

    #[test]
    fn test_parse_targeted_message_multipart_unicode() {
        let base = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [
                { "id": "prt_1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "Hello" },
                { "id": "prt_2", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "tool_use", "name": "calc" },
                { "id": "🔑", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": " World" }
            ]
        });
        let bytes = serde_json::to_vec(&base).unwrap();
        assert_eq!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", false)
                .unwrap(),
            BoundAssistantResponse::Text("Hello World".to_string())
        );
        assert!(matches!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", true).unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        ));
    }

    #[test]
    fn test_parse_targeted_message_info_metadata() {
        let base = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "Hi" }]
        });
        let bytes = serde_json::to_vec(&base).unwrap();
        assert_eq!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", false)
                .unwrap(),
            BoundAssistantResponse::Text("Hi".to_string())
        );
        assert!(matches!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", true).unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        ));

        let mut bad_role = base.clone();
        bad_role["info"]["role"] = json!("user");
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_role).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_role).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());

        let mut bad_finish = base.clone();
        bad_finish["info"]["finish"] = json!("end");
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_finish).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_finish).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());

        let mut bad_error = base.clone();
        bad_error["info"]["error"] = json!(null);
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_error).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_error).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());
    }

    #[test]
    fn test_parse_targeted_message_id_validation() {
        let base = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "Hi" }]
        });
        let bytes = serde_json::to_vec(&base).unwrap();
        assert_eq!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", false)
                .unwrap(),
            BoundAssistantResponse::Text("Hi".to_string())
        );
        assert!(matches!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", true).unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        ));

        assert!(
            parse_targeted_message(&bytes, "invalid", "msg_user", "msg_assistant", false).is_err()
        );
        assert!(
            parse_targeted_message(&bytes, "ses_scope", "invalid", "msg_assistant", false).is_err()
        );
        assert!(parse_targeted_message(&bytes, "ses_scope", "msg_user", "invalid", false).is_err());

        let mut bad_part_msgid = base.clone();
        bad_part_msgid["parts"][0]["messageID"] = json!("msg_user");
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_part_msgid).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&bad_part_msgid).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());
    }

    #[test]
    fn test_parse_targeted_message_tool_malformed_text() {
        let base = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "tool", "name": "x" }]
        });
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());

        let base2 = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text" }]
        });
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base2).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base2).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());

        let base3 = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "   " }]
        });
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base3).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&base3).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());
    }

    #[test]
    fn test_parse_targeted_message_size_limits() {
        let base = json!({
            "info": { "id": "msg_assistant", "sessionID": "ses_scope", "parentID": "msg_user", "role": "assistant", "finish": "stop" },
            "parts": [{ "id": "p1", "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "A" }]
        });
        let bytes = serde_json::to_vec(&base).unwrap();
        assert_eq!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", false)
                .unwrap(),
            BoundAssistantResponse::Text("A".to_string())
        );
        assert!(matches!(
            parse_targeted_message(&bytes, "ses_scope", "msg_user", "msg_assistant", true).unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        ));

        let mut big_text = base.clone();
        big_text["parts"][0]["text"] = json!("A".repeat(64001));
        assert!(parse_targeted_message(
            &serde_json::to_vec(&big_text).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&big_text).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());

        let mut many_parts = base.clone();
        let mut parts = Vec::new();
        for i in 0..257 {
            parts.push(json!({ "id": format!("p{}", i), "sessionID": "ses_scope", "messageID": "msg_assistant", "type": "text", "text": "A" }));
        }
        many_parts["parts"] = json!(parts);
        assert!(parse_targeted_message(
            &serde_json::to_vec(&many_parts).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            false
        )
        .is_err());
        assert!(parse_targeted_message(
            &serde_json::to_vec(&many_parts).unwrap(),
            "ses_scope",
            "msg_user",
            "msg_assistant",
            true
        )
        .is_err());
    }

    #[test]
    fn b2a1_positive_test() {
        let session = format!("ses_{}", "s".repeat(196));
        let user_msg = format!("msg_{}", "u".repeat(196));
        let assistant_msg = format!("msg_{}", "a".repeat(196));

        assert_eq!(session.len(), 200);
        assert_eq!(user_msg.len(), 200);
        assert_eq!(assistant_msg.len(), 200);

        assert!(session.starts_with("ses_"));
        assert!(user_msg.starts_with("msg_"));
        assert!(assistant_msg.starts_with("msg_"));

        assert_ne!(user_msg, assistant_msg);

        let wire = json!({
            "info": {
                "id": assistant_msg,
                "sessionID": session,
                "parentID": user_msg,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": session,
                    "messageID": assistant_msg,
                    "type": "text",
                    "text": "marker"
                }
            ]
        });

        let bytes = serde_json::to_vec(&wire).unwrap();

        let normal_res =
            parse_targeted_message(&bytes, &session, &user_msg, &assistant_msg, false).unwrap();
        assert_eq!(
            normal_res,
            BoundAssistantResponse::Text("marker".to_string())
        );

        let sensitive_res =
            parse_targeted_message(&bytes, &session, &user_msg, &assistant_msg, true).unwrap();
        assert_eq!(sensitive_res, BoundAssistantResponse::SensitiveWithheld);
    }

    #[test]
    fn b2a1_caller_priority_thinking_test() {
        let session = format!("ses_{}", "s".repeat(196));
        let user_msg = format!("msg_{}", "u".repeat(196));
        let assistant_msg = format!("msg_{}", "a".repeat(196));
        let invalid_caller = format!("bad_{}", "x".repeat(196));

        assert_eq!(session.len(), 200);
        assert_eq!(user_msg.len(), 200);
        assert_eq!(assistant_msg.len(), 200);
        assert_eq!(invalid_caller.len(), 200);

        assert!(session.starts_with("ses_"));
        assert!(user_msg.starts_with("msg_"));
        assert!(assistant_msg.starts_with("msg_"));
        assert!(!invalid_caller.starts_with("ses_"));
        assert!(!invalid_caller.starts_with("msg_"));

        assert_ne!(user_msg, assistant_msg);

        let wire = json!({
            "info": {
                "id": assistant_msg,
                "sessionID": session,
                "parentID": user_msg,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": session,
                    "messageID": assistant_msg,
                    "type": "text",
                    "text": "marker"
                }
            ]
        });

        let bytes = serde_json::to_vec(&wire).unwrap();
        let bytes_clone = bytes.clone();

        let normal_res =
            parse_targeted_message(&bytes, &session, &user_msg, &assistant_msg, false).unwrap();
        assert_eq!(
            normal_res,
            BoundAssistantResponse::Text("marker".to_string())
        );

        let sensitive_res =
            parse_targeted_message(&bytes, &session, &user_msg, &assistant_msg, true).unwrap();
        assert_eq!(sensitive_res, BoundAssistantResponse::SensitiveWithheld);

        let malformed_bytes = b"{".to_vec();
        let normal_invalid_json =
            parse_targeted_message(&malformed_bytes, &session, &user_msg, &assistant_msg, false)
                .unwrap_err()
                .to_string();
        let sensitive_invalid_json =
            parse_targeted_message(&malformed_bytes, &session, &user_msg, &assistant_msg, true)
                .unwrap_err()
                .to_string();
        assert_eq!(normal_invalid_json, "invalid json");
        assert_eq!(sensitive_invalid_json, "invalid json");

        let valid_callers = [session.clone(), user_msg.clone(), assistant_msg.clone()];
        let positions = [0, 1, 2];
        let sensitivities = [false, true];

        for pos in positions {
            for sensitive in sensitivities {
                let mut current_callers = valid_callers.clone();
                current_callers[pos] = invalid_caller.clone();
                assert_eq!(current_callers[pos], invalid_caller);

                assert_eq!(current_callers[pos].len(), 200);
                assert_eq!(current_callers[0].len(), 200);
                assert_eq!(current_callers[1].len(), 200);
                assert_eq!(current_callers[2].len(), 200);

                if pos != 0 {
                    assert_eq!(current_callers[0], valid_callers[0]);
                }
                if pos != 1 {
                    assert_eq!(current_callers[1], valid_callers[1]);
                }
                if pos != 2 {
                    assert_eq!(current_callers[2], valid_callers[2]);
                }

                let diff_count = current_callers
                    .iter()
                    .zip(valid_callers.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(diff_count, 1);

                assert_eq!(bytes, bytes_clone);

                let healthy_res = parse_targeted_message(
                    &bytes,
                    &current_callers[0],
                    &current_callers[1],
                    &current_callers[2],
                    sensitive,
                );
                let healthy_str = healthy_res.unwrap_err().to_string();

                let malformed_res = parse_targeted_message(
                    &malformed_bytes,
                    &current_callers[0],
                    &current_callers[1],
                    &current_callers[2],
                    sensitive,
                );
                let malformed_str = malformed_res.unwrap_err().to_string();

                assert_eq!(healthy_str, "invalid prefix");
                assert_eq!(malformed_str, "invalid prefix");
                assert_eq!(healthy_str, malformed_str);

                if sensitive {
                    assert_ne!(healthy_str, sensitive_invalid_json);
                } else {
                    assert_ne!(healthy_str, normal_invalid_json);
                }

                assert!(!healthy_str.contains("marker"));
                assert!(!healthy_str.contains(&session));
                assert!(!healthy_str.contains(&user_msg));
                assert!(!healthy_str.contains(&assistant_msg));
                assert!(!healthy_str.contains(&invalid_caller));
            }
        }
    }

    #[test]
    fn b2a1_decode_only_thinking_test() {
        let c1 = "ses_1234567890abcdef";
        let c2 = "msg_1234567890abcdef";
        let c3 = "msg_9876543210fedcba";
        let marker = "marker";

        validate_session_id(c1).unwrap();
        validate_message_id(c2).unwrap();
        validate_message_id(c3).unwrap();
        assert_ne!(c2, c3);

        let healthy_json = serde_json::json!({
            "info": {
                "id": c3,
                "sessionID": c1,
                "parentID": c2,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": c1,
                    "messageID": c3,
                    "type": "text",
                    "text": marker
                }
            ]
        });
        let bytes = serde_json::to_vec(&healthy_json).unwrap();
        let bytes_clone = bytes.clone();
        assert_eq!(
            parse_targeted_message(&bytes, c1, c2, c3, false).unwrap(),
            BoundAssistantResponse::Text(marker.to_string())
        );
        assert_eq!(
            parse_targeted_message(&bytes, c1, c2, c3, true).unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        );
        let malformed_bytes = b"{".to_vec();
        let invalid_utf8_bytes = vec![0xff_u8, 0xfe_u8];
        assert_eq!(malformed_bytes, b"{");
        assert!(std::str::from_utf8(&malformed_bytes).is_ok());
        assert_eq!(invalid_utf8_bytes, [0xff, 0xfe]);
        assert_eq!(invalid_utf8_bytes.len(), 2);
        assert!(std::str::from_utf8(&invalid_utf8_bytes).is_err());
        assert_eq!(bytes, bytes_clone);
        assert_ne!(bytes, malformed_bytes);
        assert_ne!(bytes, invalid_utf8_bytes);
        for sensitive in [false, true] {
            let mut cat = String::new();
            for buf in [&malformed_bytes, &invalid_utf8_bytes] {
                let err = parse_targeted_message(buf, c1, c2, c3, sensitive)
                    .unwrap_err()
                    .to_string();
                assert_eq!(err, "invalid json");
                assert!(!err.contains(marker));
                assert!(!err.contains(c1));
                assert!(!err.contains(c2));
                assert!(!err.contains(c3));
                if cat.is_empty() {
                    cat = err;
                } else {
                    assert_eq!(cat, err);
                }
            }
        }
    }

    #[test]
    fn b2a2_prefix_classes_test() {
        let session = format!("ses_{}", "s".repeat(196));
        let user_msg = format!("msg_{}", "u".repeat(196));
        let assistant_msg = format!("msg_{}", "a".repeat(196));
        assert_eq!(session.len(), 200);
        assert_eq!(user_msg.len(), 200);
        assert_eq!(assistant_msg.len(), 200);
        assert!(session.starts_with("ses_"));
        assert!(user_msg.starts_with("msg_"));
        assert!(assistant_msg.starts_with("msg_"));
        assert_ne!(user_msg, assistant_msg);

        let marker = "marker";
        let wire = json!({
            "info": {
                "id": assistant_msg,
                "sessionID": session,
                "parentID": user_msg,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": session,
                    "messageID": assistant_msg,
                    "type": "text",
                    "text": marker
                }
            ]
        });
        let envelope_bytes = serde_json::to_vec(&wire).unwrap();
        let envelope_clone = envelope_bytes.clone();

        let res_false =
            parse_targeted_message(&envelope_bytes, &session, &user_msg, &assistant_msg, false)
                .unwrap();
        let res_true =
            parse_targeted_message(&envelope_bytes, &session, &user_msg, &assistant_msg, true)
                .unwrap();
        assert_eq!(res_false, BoundAssistantResponse::Text(marker.to_string()));
        assert_eq!(res_true, BoundAssistantResponse::SensitiveWithheld);

        let callers = [session.clone(), user_msg.clone(), assistant_msg.clone()];
        let prefixes = ["ses_", "msg_", "msg_"];

        for pos in 0..3 {
            for (class_name, invalid_value) in [
                ("empty", ""),
                ("correct_prefix_empty_suffix", prefixes[pos]),
                ("wrong_prefix", "x_"),
            ] {
                match class_name {
                    "empty" => assert!(invalid_value.is_empty()),
                    "correct_prefix_empty_suffix" => {
                        assert_eq!(invalid_value, prefixes[pos]);
                        assert!(invalid_value
                            .strip_prefix(prefixes[pos])
                            .unwrap()
                            .is_empty());
                    }
                    "wrong_prefix" => {
                        assert!(!invalid_value.is_empty());
                        assert!(!invalid_value.starts_with(prefixes[pos]));
                    }
                    _ => unreachable!(),
                }

                let mut current_callers = callers.clone();
                current_callers[pos] = invalid_value.to_string();

                assert_eq!(
                    current_callers[pos], invalid_value,
                    "pos {} class {}",
                    pos, class_name
                );
                assert_eq!(
                    current_callers
                        .iter()
                        .zip(callers.iter())
                        .filter(|(a, b)| a != b)
                        .count(),
                    1,
                    "pos {} class {}",
                    pos,
                    class_name
                );
                for i in 0..3 {
                    if i != pos {
                        assert_eq!(
                            current_callers[i], callers[i],
                            "pos {} class {}",
                            pos, class_name
                        );
                    }
                }
                assert_eq!(
                    envelope_bytes, envelope_clone,
                    "pos {} class {}",
                    pos, class_name
                );

                let expected_cat = match class_name {
                    "empty" | "wrong_prefix" => "invalid prefix",
                    "correct_prefix_empty_suffix" => "empty suffix",
                    _ => unreachable!(),
                };

                let mut cats = [String::new(), String::new()];
                for (i, sensitive) in [false, true].into_iter().enumerate() {
                    let err = parse_targeted_message(
                        &envelope_bytes,
                        &current_callers[0],
                        &current_callers[1],
                        &current_callers[2],
                        sensitive,
                    )
                    .unwrap_err()
                    .to_string();
                    cats[i] = err.clone();
                    assert_eq!(
                        err, expected_cat,
                        "pos {} class {} sensitive {}",
                        pos, class_name, sensitive
                    );
                    assert!(!err.contains(marker), "pos {} class {}", pos, class_name);
                    assert!(
                        !err.contains(&callers[0]),
                        "pos {} class {}",
                        pos,
                        class_name
                    );
                    assert!(
                        !err.contains(&callers[1]),
                        "pos {} class {}",
                        pos,
                        class_name
                    );
                    assert!(
                        !err.contains(&callers[2]),
                        "pos {} class {}",
                        pos,
                        class_name
                    );
                    if !invalid_value.is_empty() {
                        assert!(
                            !err.contains(invalid_value),
                            "pos {} class {}",
                            pos,
                            class_name
                        );
                    }
                }
                assert_eq!(cats[0], cats[1], "pos {} class {}", pos, class_name);
            }
        }
    }

    #[test]
    fn b2a2_invalid_suffix_classes_test() {
        let session = format!("ses_{}", "s".repeat(196));
        let user_msg = format!("msg_{}", "u".repeat(196));
        let assistant_msg = format!("msg_{}", "a".repeat(196));

        assert_eq!(session.len(), 200);
        assert_eq!(user_msg.len(), 200);
        assert_eq!(assistant_msg.len(), 200);

        assert!(session.starts_with("ses_"));
        assert!(user_msg.starts_with("msg_"));
        assert!(assistant_msg.starts_with("msg_"));

        assert_ne!(user_msg, assistant_msg);

        let wire = json!({
            "info": {
                "id": assistant_msg,
                "sessionID": session,
                "parentID": user_msg,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": session,
                    "messageID": assistant_msg,
                    "type": "text",
                    "text": "marker"
                }
            ]
        });

        let bytes = serde_json::to_vec(&wire).unwrap();
        let bytes_clone = bytes.clone();
        let marker = "marker";

        let valid_callers = [session.clone(), user_msg.clone(), assistant_msg.clone()];

        assert_eq!(
            parse_targeted_message(
                &bytes_clone,
                &valid_callers[0],
                &valid_callers[1],
                &valid_callers[2],
                false
            )
            .unwrap(),
            BoundAssistantResponse::Text(marker.to_owned())
        );
        assert_eq!(
            parse_targeted_message(
                &bytes_clone,
                &valid_callers[0],
                &valid_callers[1],
                &valid_callers[2],
                true
            )
            .unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        );

        let prefixes = [(0usize, "ses_"), (1usize, "msg_"), (2usize, "msg_")];
        let class_defs = [
            ("non-ascii", "\u{00E9}"),
            ("control", "\u{0007}"),
            ("slash", "/"),
            ("query", "?"),
            ("hash", "#"),
            ("space", " "),
        ];

        for (pos, prefix) in prefixes {
            for (class_name, invalid_suffix) in class_defs {
                let invalid_value = format!("{}{}", prefix, invalid_suffix);

                match class_name {
                    "non-ascii" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, "é", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 2, "class {}", class_name);
                        assert!(!invalid_suffix.is_ascii(), "class {}", class_name);
                    }
                    "control" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, "\u{0007}", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 1, "class {}", class_name);
                        assert!(
                            invalid_suffix.chars().next().unwrap().is_control(),
                            "class {}",
                            class_name
                        );
                    }
                    "slash" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, "/", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 1, "class {}", class_name);
                    }
                    "query" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, "?", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 1, "class {}", class_name);
                    }
                    "hash" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, "#", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 1, "class {}", class_name);
                    }
                    "space" => {
                        assert!(!invalid_suffix.is_empty(), "class {}", class_name);
                        assert_eq!(invalid_suffix, " ", "class {}", class_name);
                        assert_eq!(invalid_suffix.len(), 1, "class {}", class_name);
                    }
                    _ => panic!("unknown class {}", class_name),
                }

                let mut test_callers = valid_callers.clone();
                test_callers[pos] = invalid_value.clone();

                assert_eq!(
                    test_callers[pos], invalid_value,
                    "pos {} class {}",
                    pos, class_name
                );
                match pos {
                    0 => {
                        assert_eq!(
                            test_callers[1], valid_callers[1],
                            "pos {} class {}",
                            pos, class_name
                        );
                        assert_eq!(
                            test_callers[2], valid_callers[2],
                            "pos {} class {}",
                            pos, class_name
                        );
                    }
                    1 => {
                        assert_eq!(
                            test_callers[0], valid_callers[0],
                            "pos {} class {}",
                            pos, class_name
                        );
                        assert_eq!(
                            test_callers[2], valid_callers[2],
                            "pos {} class {}",
                            pos, class_name
                        );
                    }
                    2 => {
                        assert_eq!(
                            test_callers[0], valid_callers[0],
                            "pos {} class {}",
                            pos, class_name
                        );
                        assert_eq!(
                            test_callers[1], valid_callers[1],
                            "pos {} class {}",
                            pos, class_name
                        );
                    }
                    _ => panic!("impossible position {}", pos),
                }

                assert_eq!(&bytes_clone, &bytes, "pos {} class {}", pos, class_name);

                let diff_count = valid_callers
                    .iter()
                    .zip(test_callers.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(diff_count, 1, "pos {} class {}", pos, class_name);

                let res_normal = parse_targeted_message(
                    &bytes_clone,
                    &test_callers[0],
                    &test_callers[1],
                    &test_callers[2],
                    false,
                );
                let res_sensitive = parse_targeted_message(
                    &bytes_clone,
                    &test_callers[0],
                    &test_callers[1],
                    &test_callers[2],
                    true,
                );

                let mut cats = [String::new(), String::new()];
                cats[0] = match res_normal {
                    Ok(_) => "normal".into(),
                    Err(e) => e.to_string(),
                };
                cats[1] = match res_sensitive {
                    Ok(_) => "sensitive".into(),
                    Err(e) => e.to_string(),
                };
                assert_eq!(cats[0], cats[1], "pos {} class {}", pos, class_name);

                let err = cats[0].as_str();
                assert_eq!(
                    err, "invalid suffix character",
                    "pos {} class {}",
                    pos, class_name
                );
                assert!(!err.contains(marker), "pos {} class {}", pos, class_name);
                assert!(
                    !err.contains(&valid_callers[0]),
                    "pos {} class {}",
                    pos,
                    class_name
                );
                assert!(
                    !err.contains(&valid_callers[1]),
                    "pos {} class {}",
                    pos,
                    class_name
                );
                assert!(
                    !err.contains(&valid_callers[2]),
                    "pos {} class {}",
                    pos,
                    class_name
                );
                assert!(
                    !err.contains(&invalid_value),
                    "pos {} class {}",
                    pos,
                    class_name
                );
            }
        }
    }

    #[test]
    fn b2a2_length_boundary_test() {
        let session = format!("ses_{}", "s".repeat(196));
        let user_msg = format!("msg_{}", "u".repeat(196));
        let assistant_msg = format!("msg_{}", "a".repeat(196));

        assert_eq!(session.len(), 200);
        assert_eq!(user_msg.len(), 200);
        assert_eq!(assistant_msg.len(), 200);

        assert!(session.starts_with("ses_"));
        assert!(user_msg.starts_with("msg_"));
        assert!(assistant_msg.starts_with("msg_"));

        assert_ne!(user_msg, assistant_msg);

        let marker = "marker";
        let wire = json!({
            "info": {
                "id": assistant_msg,
                "sessionID": session,
                "parentID": user_msg,
                "role": "assistant",
                "finish": "stop"
            },
            "parts": [
                {
                    "id": "part_1",
                    "sessionID": session,
                    "messageID": assistant_msg,
                    "type": "text",
                    "text": marker
                }
            ]
        });

        let bytes = serde_json::to_vec(&wire).unwrap();
        let bytes_clone = bytes.clone();

        let valid_callers = [session, user_msg, assistant_msg];

        assert_eq!(
            parse_targeted_message(
                &bytes,
                valid_callers[0].as_str(),
                valid_callers[1].as_str(),
                valid_callers[2].as_str(),
                false
            )
            .unwrap(),
            BoundAssistantResponse::Text(marker.to_owned())
        );
        assert_eq!(
            parse_targeted_message(
                &bytes,
                valid_callers[0].as_str(),
                valid_callers[1].as_str(),
                valid_callers[2].as_str(),
                true
            )
            .unwrap(),
            BoundAssistantResponse::SensitiveWithheld
        );

        for pos in 0..3 {
            let class_name = match pos {
                0 => "session",
                1 => "user_msg",
                2 => "assistant_msg",
                _ => unreachable!(),
            };

            let prefix = match pos {
                0 => "ses_",
                1 => "msg_",
                2 => "msg_",
                _ => unreachable!(),
            };

            let invalid_value = format!("{}{}", prefix, "x".repeat(197));
            assert_eq!(invalid_value.len(), 201);
            assert!(invalid_value.starts_with(prefix));

            let mut fresh_callers = valid_callers.clone();
            fresh_callers[pos] = invalid_value.clone();

            assert_eq!(fresh_callers[pos], invalid_value);
            if pos != 0 {
                assert_eq!(fresh_callers[0], valid_callers[0]);
            }
            if pos != 1 {
                assert_eq!(fresh_callers[1], valid_callers[1]);
            }
            if pos != 2 {
                assert_eq!(fresh_callers[2], valid_callers[2]);
            }
            assert_eq!(bytes, bytes_clone);

            let unequal_count = fresh_callers
                .iter()
                .zip(valid_callers.iter())
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(unequal_count, 1, "pos {} class {}", pos, class_name);

            let mut agreements = [String::new(), String::new()];
            for (sensitive, idx) in [(false, 0), (true, 1)] {
                let res = parse_targeted_message(
                    &bytes,
                    fresh_callers[0].as_str(),
                    fresh_callers[1].as_str(),
                    fresh_callers[2].as_str(),
                    sensitive,
                );
                let err = res.unwrap_err().to_string();
                assert_eq!(err, "suffix too long", "pos {} class {}", pos, class_name);
                agreements[idx] = err;
            }
            assert_eq!(
                agreements[0], agreements[1],
                "pos {} class {}",
                pos, class_name
            );

            assert!(
                !agreements[0].contains(marker),
                "pos {} class {}",
                pos,
                class_name
            );
            assert!(
                !agreements[0].contains(&valid_callers[0]),
                "pos {} class {}",
                pos,
                class_name
            );
            assert!(
                !agreements[0].contains(&valid_callers[1]),
                "pos {} class {}",
                pos,
                class_name
            );
            assert!(
                !agreements[0].contains(&valid_callers[2]),
                "pos {} class {}",
                pos,
                class_name
            );
            assert!(
                !agreements[0].contains(&invalid_value),
                "pos {} class {}",
                pos,
                class_name
            );
        }
    }
}
