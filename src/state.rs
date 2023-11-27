use crate::hos_connection::HOSConnection;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub hos_connections: RwLock<HashMap<String, Arc<Mutex<HOSConnection>>>>,
    pub should_block: bool,
    pub allowed_ip: String,
    pub required_passwd: Option<String>,
}

pub async fn prune_with_mut_hashmap(hashmap: &mut HashMap<String, Arc<Mutex<HOSConnection>>>) {
    let mut dead_keys: Vec<String> = vec![];
    for key in hashmap.keys() {
        let mut connection = hashmap.get(key).unwrap().lock().await;
        let dead = connection.dead || connection.session.ping(b"").await.is_err();
        drop(connection);
        if dead {
            dead_keys.push(key.to_string());
        }
    }
    log::info!("Pruning {} dead connection(s)", dead_keys.clone().len());
    for dead_key in dead_keys {
        hashmap.remove(&dead_key);
    }
}

impl AppState {
    pub async fn prune(&mut self) {
        let mut connections = self.hos_connections.write();
        prune_with_mut_hashmap(&mut connections).await;
    }
}
