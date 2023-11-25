use crate::{hos_connection::HOSConnection, json::HOSIncomingReq};
use actix_web::web::Data;
use actix_ws::{Message, MessageStream, Session};
use futures_util::stream::StreamExt;
use std::sync::Arc;
use uuid::Uuid;

pub async fn hos_ws(
    mut session: Session,
    mut msg_stream: MessageStream,
    data: Data<crate::state::AppState>,
) {
    // the pairing_code and the connection_id are not the same
    // pairing_code is like a password, connection_id is used exclusively internally serverside to identify connections
    let mut pairing_code: Option<String> = None;
    let connection_id: Uuid = Uuid::new_v4();

    log::info!("Connected to session ID {}", connection_id.to_string());

    let mut write = data.hos_connections.write();

    write.insert(
        connection_id.to_string(),
        Arc::new(
            HOSConnection {
                incoming: vec![],
                session: debug_ignore::DebugIgnore(session.clone()),
                pairing_code: pairing_code.clone(),
                connection_id,
            }
            .into(),
        ),
    );

    drop(write);

    let close_reason = loop {
        match msg_stream.next().await {
            Some(Ok(msg)) => {
                match msg {
                    Message::Text(text) => match serde_json::from_str(&text) {
                        Ok(request) => {
                            let incoming: HOSIncomingReq = request;
                            match incoming._type.as_str() {
                                "pairing" => {
                                    pairing_code = incoming.code;
                                    let mut write = data.hos_connections.write();
                                    write
                                        .get_mut(&connection_id.to_string())
                                        .unwrap()
                                        .lock()
                                        .await
                                        .pairing_code = pairing_code.clone();
                                    log::info!(
                                        "Paired with pairing code {}, session ID {}",
                                        pairing_code
                                            .clone()
                                            .unwrap_or("[no pairing code]".to_string()),
                                        connection_id.to_string()
                                    );
                                    drop(write);
                                }
                                "response" => {
                                    log::info!("Response received from pairing code {}, request ID {}, session ID {}",
                                            pairing_code.clone().unwrap_or("[no pairing code]".to_string()),
                                            incoming.clone().id.unwrap_or("[no id]".to_string()),
                                            connection_id.to_string()
                                        );
                                    let mut conn = data.hos_connections.write();
                                    conn.get_mut(&connection_id.to_string())
                                        .unwrap()
                                        .lock()
                                        .await
                                        .incoming
                                        .push(incoming.clone());
                                }
                                _ => {}
                            }
                        }
                        Err(err) => {
                            log::error!(
                                "Error deserializing a request for session ID {}",
                                connection_id.to_string()
                            );
                            log::debug!("{}", err);
                        }
                    },

                    Message::Close(reason) => {
                        break reason;
                    }

                    Message::Ping(bytes) => {
                        let _ = session.pong(&bytes).await;
                    }

                    Message::Pong(_) => {}

                    // no-op; ignore
                    Message::Nop => {}

                    _ => {}
                };
            }

            // error or end of stream
            _ => break None,
        }
    };

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;

    data.hos_connections
        .write()
        .remove(&connection_id.to_string());

    log::info!("Disconnected from session ID {}", connection_id.to_string());
}
