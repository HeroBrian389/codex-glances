use chrono::{DateTime, Utc};

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
