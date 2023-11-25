use crate::hos_connection::HOSConnection;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub hos_connections: RwLock<HashMap<String, Arc<Mutex<HOSConnection>>>>,
    pub should_block: bool,
    pub allowed_ip: String,
}
