use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SessionStatus {
    Running,
    WaitingInput,
    Idle,
    Unknown,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::WaitingInput => "WAITING",
            Self::Idle => "IDLE",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::WaitingInput => 0,
            Self::Running => 1,
            Self::Idle => 2,
            Self::Unknown => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub screen_id: String,
    pub screen_name: String,
    pub branch: String,
    pub cwd: String,
    pub thread_id: String,
    pub status: SessionStatus,
    pub needs_attention: bool,
    pub scheduled_follow_ups: usize,
    pub last_event: String,
    pub last_user: String,
    pub last_agent: String,
    pub last_update: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceRow {
    pub key: String,
    pub path: String,
    pub display_name: String,
    pub branch_label: String,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub session_count: usize,
    pub waiting_sessions: usize,
    pub running_sessions: usize,
    pub follow_ups: usize,
    pub last_update: Option<DateTime<Utc>>,
    pub last_user: String,
    pub last_agent: String,
    pub sessions: Vec<SessionRow>,
}

#[derive(Debug, Clone)]
pub struct DashboardData {
    pub workspaces: Vec<WorkspaceRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceRegistry {
    #[serde(default)]
    pub workspaces: Vec<StoredWorkspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredWorkspace {
    pub path: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
}
