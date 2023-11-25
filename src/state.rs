use crate::hos_connection::HOSConnection;
use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct AppState {
    pub hos_connections: Mutex<HashMap<String, HOSConnection>>,
    pub should_block: bool,
    pub allowed_ip: String,
}
