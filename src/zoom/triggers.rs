use crate::n8n::{Workflow, WorkflowNode};

/// Extracted configuration from a Zoom Trigger node
#[derive(Debug, Clone)]
pub struct ZoomTriggerConfig {
    /// The n8n webhook ID (from `node.webhookId`) used as the correlation key
    pub webhook_id: String,

    /// The workflow ID this trigger belongs to
    pub workflow_id: String,

    /// The workflow name for logging
    pub workflow_name: String,

    /// Whether the workflow is active (triggers enabled in n8n)
    pub workflow_active: bool,

    /// The event types this trigger listens for (may include `"*"` for all allowlisted events)
    pub events: Vec<String>,
}

/// Parse Zoom Trigger configuration from a workflow node
pub fn parse_zoom_trigger(workflow: &Workflow, node: &WorkflowNode) -> Option<ZoomTriggerConfig> {
    if node.node_type != "n8n-nodes-unihook-zoom-trigger.zoomTrigger" {
        return None;
    }

    let params = &node.parameters;

    let events: Vec<String> = params
        .get("event")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let webhook_id = node.webhook_id.as_ref()?;

    Some(ZoomTriggerConfig {
        webhook_id: webhook_id.clone(),
        workflow_id: workflow.id.clone(),
        workflow_name: workflow.name.clone(),
        workflow_active: workflow.active,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_workflow(id: &str, name: &str, nodes: Vec<WorkflowNode>) -> Workflow {
        Workflow {
            id: id.to_string(),
            name: name.to_string(),
            active: true,
            nodes,
            static_data: None,
        }
    }

    fn create_zoom_trigger_node(
        webhook_id: Option<&str>,
        params: serde_json::Value,
    ) -> WorkflowNode {
        WorkflowNode {
            node_type: "n8n-nodes-unihook-zoom-trigger.zoomTrigger".to_string(),
            name: "Zoom Trigger".to_string(),
            parameters: params,
            webhook_id: webhook_id.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_parse_zoom_trigger_basic() {
        let node =
            create_zoom_trigger_node(Some("webhook-z1"), json!({ "event": ["meeting.started"] }));
        let workflow = create_workflow("wf1", "Zoom Workflow", vec![node.clone()]);

        let config = parse_zoom_trigger(&workflow, &node).unwrap();

        assert_eq!(config.webhook_id, "webhook-z1");
        assert_eq!(config.events, vec!["meeting.started"]);
    }

    #[test]
    fn test_parse_zoom_trigger_multiple_events() {
        let node = create_zoom_trigger_node(
            Some("webhook-z2"),
            json!({ "event": ["meeting.started", "meeting.ended"] }),
        );
        let workflow = create_workflow("wf2", "Multi Event", vec![node.clone()]);

        let config = parse_zoom_trigger(&workflow, &node).unwrap();

        assert_eq!(config.events, vec!["meeting.started", "meeting.ended"]);
    }

    #[test]
    fn test_parse_zoom_trigger_wildcard() {
        let node = create_zoom_trigger_node(Some("webhook-z3"), json!({ "event": ["*"] }));
        let workflow = create_workflow("wf3", "Wildcard", vec![node.clone()]);

        let config = parse_zoom_trigger(&workflow, &node).unwrap();

        assert_eq!(config.events, vec!["*"]);
    }

    #[test]
    fn test_parse_non_zoom_node_returns_none() {
        let node = WorkflowNode {
            node_type: "n8n-nodes-base.httpRequest".to_string(),
            name: "HTTP Request".to_string(),
            parameters: json!({}),
            webhook_id: Some("webhook-123".to_string()),
        };
        let workflow = create_workflow("wf1", "Workflow", vec![node.clone()]);

        assert!(parse_zoom_trigger(&workflow, &node).is_none());
    }

    #[test]
    fn test_parse_node_without_webhook_id() {
        let node = create_zoom_trigger_node(None, json!({ "event": ["meeting.started"] }));
        let workflow = create_workflow("wf1", "Workflow", vec![node.clone()]);

        assert!(parse_zoom_trigger(&workflow, &node).is_none());
    }
}
