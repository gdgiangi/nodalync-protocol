use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub network_id: String,
    pub listen_addr: String,
    pub data_dir: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub address: String,
    pub connected_since: String,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentInfo {
    pub content_id: String,
    pub creator_id: String,
    pub title: String,
    pub size_bytes: u64,
    pub created_at: String,
    pub attribution_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentChannel {
    pub channel_id: String,
    pub counterparty: String,
    pub balance: String,
    pub status: ChannelStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelStatus {
    Opening,
    Active,
    Closing,
    Closed,
    Disputed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub settlement_id: String,
    pub amount: String,
    pub recipient: String,
    pub status: SettlementStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SettlementStatus {
    Pending,
    Processing,
    Confirmed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppNotification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}