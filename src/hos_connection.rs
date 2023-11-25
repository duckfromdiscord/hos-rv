use crate::json::hos_request;
use crate::json::HOSIncomingReq;
use actix_ws::Session;
use debug_ignore::DebugIgnore;
use uuid::Uuid;

pub struct HOSBackend {
    pub sess: Uuid,
}

#[derive(Debug)]
pub struct HOSConnection {
    pub incoming: Vec<HOSIncomingReq>,
    pub session: DebugIgnore<Session>,
    pub pairing_code: Option<String>,
    pub connection_id: Uuid,
}

impl HOSConnection {
    pub async fn req(&mut self, method: &str, url: &str) -> Result<Uuid, serde_json::Error> {
        let request_id: Uuid = Uuid::new_v4();
        let request_text = hos_request(method, url, request_id.to_string());
        match request_text {
            Ok(text) => {
                self.session.text(text).await.unwrap();
            }
            Err(err) => {
                return Err(err);
            }
        }
        Ok(request_id)
    }
}
