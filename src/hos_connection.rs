use actix_ws::Session;
use crate::json::HOSIncomingReq;
use crate::json::hos_request;
use crossbeam::channel::{unbounded, Receiver};
use std::collections::HashMap;
use uuid::Uuid;

pub struct HOSBackend {
    pub sess: Uuid,
}

pub struct HOSConnection {
    pub incoming: Vec<HOSIncomingReq>,
    pub session: Session,
    pub pairing_code: Option<String>,
    pub channels: HashMap<String, crossbeam::channel::Sender<String>>,
    pub connection_id: Uuid,
}

impl HOSConnection {
    pub async fn req(
        &mut self,
        method: &str,
        url: &str,
    ) -> Result<Receiver<String>, serde_json::Error> {
        let request_id: Uuid = Uuid::new_v4();
        let (s, r) = unbounded();
        self.channels.insert(request_id.to_string(), s);
        let request_text = hos_request(method, url, request_id.to_string());
        match request_text {
            Ok(text) => {
                self.session.text(text).await.unwrap();
            }
            Err(err) => {
                return Err(err);
            }
        }
        Ok(r)
    }
}
