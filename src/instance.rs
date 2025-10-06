use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceType {
    Vm,
    Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: Uuid,
    pub name: String,
    pub instance_type: InstanceType,
    pub status: InstanceStatus,
    pub pid: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub network: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
}

impl Instance {
    pub fn new(name: String, instance_type: InstanceType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            instance_type,
            status: InstanceStatus::Stopped,
            pid: None,
            created_at: now,
            last_updated: now,
            cpu_cores: 2,
            memory_mb: 1024,
            network: None,
            ip_address: None,
        }
    }

    pub fn update_status(&mut self, status: InstanceStatus) {
        self.status = status;
        self.last_updated = Utc::now();
    }

    pub fn set_pid(&mut self, pid: Option<u32>) {
        self.pid = pid;
        self.last_updated = Utc::now();
    }

    pub fn set_ip_address(&mut self, ip: Option<String>) {
        self.ip_address = ip;
        self.last_updated = Utc::now();
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, InstanceStatus::Running)
    }

    pub fn display_name(&self) -> &str {
        &self.name
    }
}
