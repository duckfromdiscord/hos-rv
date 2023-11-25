use std::collections::HashMap;
use crate::{json::HOSIncomingReq, hos_connection::HOSConnection};
use actix_web::web::Data;
use actix_ws::{Message, MessageStream, Session};
use futures_util::stream::StreamExt;
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

    data.hos_connections.lock().await.insert(
        connection_id.to_string(),
        HOSConnection {
            incoming: vec![],
            session: session.clone(),
            pairing_code: pairing_code.clone(),
            channels: HashMap::new(),
            connection_id,
        },
    );

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
                                    data.hos_connections.lock().await.get_mut(
                                        &connection_id.to_string(),
                                    ).unwrap().pairing_code = pairing_code.clone();
                                    log::info!(
                                        "Paired with pairing code {}, session ID {}",
                                        pairing_code
                                            .clone()
                                            .unwrap_or("[no pairing code]".to_string()),
                                        connection_id.to_string()
                                    );
                                }
                                "response" => {
                                    log::info!("Response received from pairing code {}, request ID {}, session ID {}",
                                            pairing_code.clone().unwrap_or("[no pairing code]".to_string()),
                                            incoming.clone().id.unwrap_or("[no id]".to_string()),
                                            connection_id.to_string()
                                        );
                                    let mut conn = data.hos_connections.lock().await;
                                    conn.get_mut(&connection_id.to_string())
                                        .unwrap()
                                        .incoming
                                        .push(incoming.clone());
                                    match incoming.content {
                                        Some(ref content) => {
                                            if let Some(channel) = conn
                                                .get_mut(&connection_id.to_string())
                                                .unwrap()
                                                .channels
                                                .get_mut(&incoming.clone().id.unwrap())
                                            {
                                                log::info!("Channel for request id {} found in session ID {}",
                                                    incoming.clone().id.unwrap_or("[no id]".to_string()),
                                                    connection_id.to_string()
                                                );
                                                channel.send(content.to_string());
                                            } else {
                                                log::warn!("Channel for request id {} not found in session ID {}",
                                                    incoming.clone().id.unwrap_or("[no id]".to_string()),
                                                    connection_id.to_string()
                                                );
                                            }
                                        }
                                        None => {
                                            log::warn!(
                                                "Content-less response in session ID {}",
                                                connection_id.to_string()
                                            );
                                        }
                                    }
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

    data.hos_connections.lock().await.remove(&connection_id.to_string());

    log::info!("Disconnected from session ID {}", connection_id.to_string());
}
