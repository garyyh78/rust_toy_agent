//! team_protocols.rs - Team Protocols for shutdown and plan approval
//!
//! Shutdown protocol and plan approval protocol, both using the same
//! request_id correlation pattern. Uses DashMap for lock-free concurrent
//! request tracking.
//!
//! ```text
//!     Shutdown FSM: pending -> approved | rejected
//!
//!     Lead                              Teammate
//!     shutdown_request  ------------->  receives request
//!     {request_id: abc}                decides: approve?
//!                                        |
//!     shutdown_response <-------------  shutdown_response
//!     {request_id: abc}                {request_id: abc,
//!      approve: true}                   approve: true}
//!        |
//!     status -> "shutdown"
//!
//!     Plan approval FSM: pending -> approved | rejected
//!
//!     Teammate                          Lead
//!     plan_approval  --------------->  reviews plan text
//!     submit: {plan:"..."}             approve/reject?
//!                                        |
//!     plan_approval_resp <------------  plan_approval
//!     {approve: true}                  review: {req_id,
//!                                       approve: true}
//! ```
//!
//! Key insight: "Same request_id correlation pattern, two domains."

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Status of a protocol request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    Pending,
    Approved,
    Rejected,
}

/// A shutdown request tracked by request_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRequest {
    pub request_id: String,
    pub target: String,
    pub status: RequestStatus,
}

/// A plan approval request tracked by request_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequest {
    pub request_id: String,
    pub from: String,
    pub plan: String,
    pub status: RequestStatus,
}

/// Thread-safe request trackers using DashMap.
#[derive(Clone)]
pub struct ProtocolTracker {
    pub shutdown_requests: Arc<DashMap<String, ShutdownRequest>>,
    pub plan_requests: Arc<DashMap<String, PlanRequest>>,
}

impl ProtocolTracker {
    pub fn new() -> Self {
        Self {
            shutdown_requests: Arc::new(DashMap::new()),
            plan_requests: Arc::new(DashMap::new()),
        }
    }

    // -- Shutdown protocol --

    /// Create a shutdown request and return the request_id.
    pub fn create_shutdown_request(&self, target: &str) -> String {
        // Generate a short UUID as correlation key between request and response
        let request_id = Uuid::new_v4().to_string()[..8].to_string();
        self.shutdown_requests.insert(
            request_id.clone(),
            ShutdownRequest {
                request_id: request_id.clone(),
                target: target.to_string(),
                status: RequestStatus::Pending,
            },
        );
        request_id
    }

    /// Respond to a shutdown request. Returns the updated status.
    pub fn respond_shutdown(&self, request_id: &str, approve: bool) -> Result<String, String> {
        // DashMap::get_mut gives a write guard; held until scope exits
        let mut req = self
            .shutdown_requests
            .get_mut(request_id)
            .ok_or_else(|| format!("Unknown shutdown request_id '{request_id}'"))?;
        // Transition from pending to approved/rejected
        req.status = if approve {
            RequestStatus::Approved
        } else {
            RequestStatus::Rejected
        };
        Ok(format!(
            "Shutdown {} for '{}'",
            if approve { "approved" } else { "rejected" },
            req.target
        ))
    }

    /// Check the status of a shutdown request.
    pub fn check_shutdown(&self, request_id: &str) -> Option<ShutdownRequest> {
        self.shutdown_requests.get(request_id).map(|r| r.clone())
    }

    // -- Plan approval protocol --

    /// Submit a plan for approval. Returns the request_id.
    pub fn submit_plan(&self, from: &str, plan: &str) -> String {
        let request_id = Uuid::new_v4().to_string()[..8].to_string();
        self.plan_requests.insert(
            request_id.clone(),
            PlanRequest {
                request_id: request_id.clone(),
                from: from.to_string(),
                plan: plan.to_string(),
                status: RequestStatus::Pending,
            },
        );
        request_id
    }

    /// Review (approve/reject) a plan. Returns the decision.
    pub fn review_plan(
        &self,
        request_id: &str,
        approve: bool,
        feedback: &str,
    ) -> Result<String, String> {
        let mut req = self
            .plan_requests
            .get_mut(request_id)
            .ok_or_else(|| format!("Unknown plan request_id '{request_id}'"))?;
        req.status = if approve {
            RequestStatus::Approved
        } else {
            RequestStatus::Rejected
        };
        let status_str = if approve { "approved" } else { "rejected" };
        let feedback_part = if feedback.is_empty() {
            String::new()
        } else {
            format!(" Feedback: {feedback}")
        };
        Ok(format!(
            "Plan {status_str} for '{}'{}",
            req.from, feedback_part
        ))
    }

    /// Check the status of a plan request.
    pub fn check_plan(&self, request_id: &str) -> Option<PlanRequest> {
        self.plan_requests.get(request_id).map(|r| r.clone())
    }

    /// Get all shutdown requests (for debugging/listing).
    pub fn list_shutdown_requests(&self) -> Vec<ShutdownRequest> {
        self.shutdown_requests
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get all plan requests (for debugging/listing).
    pub fn list_plan_requests(&self) -> Vec<PlanRequest> {
        self.plan_requests
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }
}

impl Default for ProtocolTracker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_tracker_creation() {
        let tracker = ProtocolTracker::new();
        assert!(tracker.list_shutdown_requests().is_empty());
        assert!(tracker.list_plan_requests().is_empty());
    }

    // -- Shutdown tests --

    #[test]
    fn test_create_shutdown_request() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.create_shutdown_request("alice");
        assert_eq!(req_id.len(), 8);

        let req = tracker.check_shutdown(&req_id).unwrap();
        assert_eq!(req.target, "alice");
        assert_eq!(req.status, RequestStatus::Pending);
    }

    #[test]
    fn test_respond_shutdown_approve() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.create_shutdown_request("alice");

        let result = tracker.respond_shutdown(&req_id, true).unwrap();
        assert!(result.contains("approved"));
        assert!(result.contains("alice"));

        let req = tracker.check_shutdown(&req_id).unwrap();
        assert_eq!(req.status, RequestStatus::Approved);
    }

    #[test]
    fn test_respond_shutdown_reject() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.create_shutdown_request("bob");

        let result = tracker.respond_shutdown(&req_id, false).unwrap();
        assert!(result.contains("rejected"));

        let req = tracker.check_shutdown(&req_id).unwrap();
        assert_eq!(req.status, RequestStatus::Rejected);
    }

    #[test]
    fn test_respond_shutdown_unknown_id() {
        let tracker = ProtocolTracker::new();
        let result = tracker.respond_shutdown("nonexistent", true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown shutdown request_id"));
    }

    #[test]
    fn test_list_shutdown_requests() {
        let tracker = ProtocolTracker::new();
        tracker.create_shutdown_request("alice");
        tracker.create_shutdown_request("bob");

        let requests = tracker.list_shutdown_requests();
        assert_eq!(requests.len(), 2);
    }

    // -- Plan approval tests --

    #[test]
    fn test_submit_plan() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.submit_plan("alice", "Implement auth module");
        assert_eq!(req_id.len(), 8);

        let req = tracker.check_plan(&req_id).unwrap();
        assert_eq!(req.from, "alice");
        assert_eq!(req.plan, "Implement auth module");
        assert_eq!(req.status, RequestStatus::Pending);
    }

    #[test]
    fn test_review_plan_approve() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.submit_plan("alice", "Refactor DB layer");

        let result = tracker.review_plan(&req_id, true, "Looks good!").unwrap();
        assert!(result.contains("approved"));
        assert!(result.contains("alice"));
        assert!(result.contains("Looks good!"));

        let req = tracker.check_plan(&req_id).unwrap();
        assert_eq!(req.status, RequestStatus::Approved);
    }

    #[test]
    fn test_review_plan_reject() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.submit_plan("bob", "Delete everything");

        let result = tracker.review_plan(&req_id, false, "Too risky").unwrap();
        assert!(result.contains("rejected"));
        assert!(result.contains("Too risky"));

        let req = tracker.check_plan(&req_id).unwrap();
        assert_eq!(req.status, RequestStatus::Rejected);
    }

    #[test]
    fn test_review_plan_no_feedback() {
        let tracker = ProtocolTracker::new();
        let req_id = tracker.submit_plan("alice", "Add tests");

        let result = tracker.review_plan(&req_id, true, "").unwrap();
        assert!(result.contains("approved"));
        assert!(!result.contains("Feedback"));
    }

    #[test]
    fn test_review_plan_unknown_id() {
        let tracker = ProtocolTracker::new();
        let result = tracker.review_plan("nonexistent", true, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown plan request_id"));
    }

    #[test]
    fn test_list_plan_requests() {
        let tracker = ProtocolTracker::new();
        tracker.submit_plan("alice", "Plan A");
        tracker.submit_plan("bob", "Plan B");

        let requests = tracker.list_plan_requests();
        assert_eq!(requests.len(), 2);
    }

    // -- Concurrent access tests (DashMap advantage) --

    #[test]
    fn test_concurrent_shutdown_requests() {
        use std::thread;
        let tracker = ProtocolTracker::new();
        let mut handles = vec![];

        for i in 0..10 {
            let tracker_clone = tracker.clone();
            handles.push(thread::spawn(move || {
                tracker_clone.create_shutdown_request(&format!("agent_{i}"));
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(tracker.list_shutdown_requests().len(), 10);
    }

    #[test]
    fn test_concurrent_plan_submissions() {
        use std::thread;
        let tracker = ProtocolTracker::new();
        let mut handles = vec![];

        for i in 0..10 {
            let tracker_clone = tracker.clone();
            handles.push(thread::spawn(move || {
                tracker_clone.submit_plan(&format!("agent_{i}"), &format!("Plan {i}"));
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(tracker.list_plan_requests().len(), 10);
    }

    // -- Serialization tests --

    #[test]
    fn test_request_status_serialization() {
        assert_eq!(
            serde_json::to_string(&RequestStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&RequestStatus::Approved).unwrap(),
            "\"approved\""
        );
        assert_eq!(
            serde_json::to_string(&RequestStatus::Rejected).unwrap(),
            "\"rejected\""
        );
    }

    #[test]
    fn test_shutdown_request_serialization() {
        let req = ShutdownRequest {
            request_id: "abc12345".to_string(),
            target: "alice".to_string(),
            status: RequestStatus::Pending,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ShutdownRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id, "abc12345");
        assert_eq!(parsed.target, "alice");
    }

    #[test]
    fn test_plan_request_serialization() {
        let req = PlanRequest {
            request_id: "def67890".to_string(),
            from: "bob".to_string(),
            plan: "Build feature X".to_string(),
            status: RequestStatus::Approved,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: PlanRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from, "bob");
        assert_eq!(parsed.plan, "Build feature X");
    }
}
