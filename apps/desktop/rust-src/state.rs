use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub auto_start_node: bool,
    pub minimize_to_tray: bool,
    pub check_updates: bool,
    pub log_level: String,
    pub refresh_interval: u64, // seconds
    pub theme: String, // "dark", "light", "auto"
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_start_node: false,
            minimize_to_tray: true,
            check_updates: true,
            log_level: "info".to_string(),
            refresh_interval: 30,
            theme: "dark".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub settings: Arc<Mutex<AppSettings>>,
    pub notifications: Arc<Mutex<Vec<AppNotification>>>,
    pub last_node_status: Arc<Mutex<Option<NodeStatus>>>,
    pub last_update: Arc<Mutex<Option<DateTime<Utc>>>>,
    pub cache: Arc<Mutex<HashMap<String, (DateTime<Utc>, serde_json::Value)>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(Mutex::new(AppSettings::default())),
            notifications: Arc::new(Mutex::new(Vec::new())),
            last_node_status: Arc::new(Mutex::new(None)),
            last_update: Arc::new(Mutex::new(None)),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn add_notification(&self, notification: AppNotification) {
        if let Ok(mut notifications) = self.notifications.lock() {
            notifications.push(notification);
            // Keep only the last 100 notifications
            if notifications.len() > 100 {
                notifications.remove(0);
            }
        }
    }
    
    pub fn get_notifications(&self) -> Vec<AppNotification> {
        self.notifications.lock()
            .map(|n| n.clone())
            .unwrap_or_else(|_| Vec::new())
    }
    
    pub fn clear_notifications(&self) {
        if let Ok(mut notifications) = self.notifications.lock() {
            notifications.clear();
        }
    }
    
    pub fn update_node_status(&self, status: NodeStatus) {
        if let Ok(mut last_status) = self.last_node_status.lock() {
            *last_status = Some(status);
        }
        if let Ok(mut last_update) = self.last_update.lock() {
            *last_update = Some(Utc::now());
        }
    }
    
    pub fn get_cached_value(&self, key: &str, max_age_seconds: u64) -> Option<serde_json::Value> {
        if let Ok(cache) = self.cache.lock() {
            if let Some((timestamp, value)) = cache.get(key) {
                let age = Utc::now().signed_duration_since(*timestamp);
                if age.num_seconds() < max_age_seconds as i64 {
                    return Some(value.clone());
                }
            }
        }
        None
    }
    
    pub fn cache_value(&self, key: String, value: serde_json::Value) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, (Utc::now(), value));
        }
    }
    
    pub fn get_settings(&self) -> AppSettings {
        self.settings.lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| AppSettings::default())
    }
    
    pub fn update_settings(&self, settings: AppSettings) {
        if let Ok(mut app_settings) = self.settings.lock() {
            *app_settings = settings;
        }
    }
}