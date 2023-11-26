use actix_web::get;
use actix_web::{
    http::{header::ContentType, StatusCode},
    middleware, rt,
    web::{self, Data},
    App, Error, HttpRequest, HttpResponse, HttpServer,
};
use base64::{engine::general_purpose, Engine as _};
use clap::{Arg, Command};
use hos_rv::state::{prune_with_mut_hashmap, AppState};
use hos_rv::{json::HOSConnectionList, ws_handler};
use std::{collections::HashMap, time::Duration};
use tokio::sync::broadcast;
use uuid::Uuid;

async fn hos_ws_route(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;
    rt::spawn(ws_handler::hos_ws(session, msg_stream, data));

    Ok(res)
}

fn is_allowed(req: HttpRequest, data: Data<AppState>) -> bool {
    match &data.required_passwd {
        Some(required_passwd) => match req.headers().get("HOS-PASSWD") {
            Some(supplied_passwd) => {
                return *required_passwd
                    == String::from_utf8(supplied_passwd.as_bytes().to_vec()).unwrap();
            }
            None => false,
        },
        None => {
            if data.should_block == false {
                return true;
            }
            if let Some(val) = req.peer_addr() {
                val.ip().to_string() == data.allowed_ip
            } else {
                false
            }
        }
    }
}

fn full_query_path(req: HttpRequest, path: &str) -> String {
    match req.query_string().is_empty() {
        true => path.to_string(),
        false => path.to_string() + "?" + req.query_string(),
    }
}

#[get("/list")]
pub async fn handle_list_req(req: HttpRequest, data: Data<AppState>) -> HttpResponse {
    if !(is_allowed(req.clone(), data.clone())) {
        return HttpResponse::build(StatusCode::UNAUTHORIZED).into();
    }

    let mut connections: Vec<(String, String)> = vec![];
    let conns = data.hos_connections.read();
    for conn in conns.values() {
        let conn = conn.lock().await;
        connections.push((
            conn.connection_id.to_string(),
            conn.pairing_code.clone().unwrap_or("".to_string()),
        ));
    }

    let connection_list = HOSConnectionList { connections };

    HttpResponse::build(StatusCode::OK)
        .content_type(ContentType::json())
        .body(serde_json::to_string(&connection_list).unwrap())
}

#[get("sid/{session_id}/{tail:.*}")]
pub async fn handle_sid_get(
    req: HttpRequest,
    info: web::Path<(String, String)>,
    data: Data<AppState>,
) -> HttpResponse {
    if !(is_allowed(req.clone(), data.clone())) {
        return HttpResponse::build(StatusCode::UNAUTHORIZED).into();
    }

    let connection_id = &info.0;
    let path = full_query_path(req, &info.1);

    let request_id: Uuid;
    let dead: bool;

    let mut conns = data.hos_connections.write();
    if let Some(conn) = conns.get_mut(connection_id) {
        // we have to detect dead connections here
        // because this is when we have a write handle
        // and also when we send a message.
        // we can only detect a dead connection when sending a message fails
        (request_id, dead) = conn.lock().await.req("GET", &path).await.unwrap().clone();
        drop(conns);
    } else {
        return HttpResponse::build(StatusCode::NOT_FOUND).into();
    };

    // skip to pruning if we can't talk to the connection matching our requested session id
    // otherwise we have to waste time sleeping and retrying
    if !dead {
        for _ in 1..=10 {
            tokio::time::sleep(Duration::new(1, 0)).await;
            let conns = data.hos_connections.read();
            let conn = conns.get(connection_id).unwrap();
            for x in &conn.lock().await.incoming {
                if x.clone().id.unwrap_or("".to_string()) == request_id.to_string() {
                    let b64 = x.content.clone().unwrap().to_string();
                    let bytes = general_purpose::STANDARD.decode(b64).unwrap();
                    return HttpResponse::build(StatusCode::OK)
                        .content_type(ContentType::json())
                        .body(bytes);
                }
            }
            drop(conns);
        }
    }

    // prune with a write handle so that we can both detect and remove dead connections
    prune_with_mut_hashmap(&mut data.hos_connections.write()).await;
    return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).into();
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let matches = Command::new("hos-rv")
        .arg(Arg::new("ip").short('i').value_name("IP").help("Listen IP"))
        .arg(
            Arg::new("port")
                .short('p')
                .value_name("PORT")
                .help("Listen Port"),
        )
        .arg(
            Arg::new("block")
                .short('b')
                .value_name("BLOCK")
                .num_args(0)
                .help("Only allow requests from 127.0.0.1 or provided IP"),
        )
        .arg(
            Arg::new("block_addr")
                .short('a')
                .value_name("BLOCK_ADDR")
                .help("Optional, the IP you choose to whitelist"),
        )
        .arg(
            Arg::new("lock_passwd")
            .short('l')
            .value_name("LOCK_PASSWD")
            .env("HOS_LOCK_PASSWD")
            .help("An optional password to be required in headers to access lists. Only supply if you want to allow certain requests to accept a header \"HOS-PASSWD\" of a certain value. Will ignore `-b`."),
        )
        .get_matches();

    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let listen_ip: String = matches
        .get_one::<String>("ip")
        .unwrap_or(&"127.0.0.1".to_string())
        .to_string();

    let listen_port: u16 = matches
        .get_one::<String>("port")
        .unwrap_or(&"9003".to_string())
        .parse::<u16>()
        .expect("Invalid port");

    let should_block: bool = matches.get_flag("block");

    let allowed_ip: String = matches
        .get_one::<String>("block_addr")
        .unwrap_or(&"127.0.0.1".to_string())
        .to_string();

    let required_passwd: Option<String> = matches
        .get_one::<String>("lock_passwd")
        .map(|x| x.to_string());

    if required_passwd.clone().is_some() {
        log::warn!(
            "Using a HOS lock password from either the command line or an environment variable."
        );
    }

    log::info!(
        "starting HTTP server at http://{}:{}",
        listen_ip,
        listen_port
    );

    if should_block {
        log::warn!("Blocking all non-{} requests", allowed_ip);
    } else {
        log::warn!("Responding to requests from any address. Remember to run with `-b` to block requests that don't come from {}.", allowed_ip);
    }

    let (tx, _) = broadcast::channel::<web::Bytes>(128);

    let state = web::Data::new(AppState {
        hos_connections: HashMap::new().into(),
        should_block,
        allowed_ip,
        required_passwd,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(handle_list_req)
            .service(handle_sid_get)
            .service(web::resource("/ws").route(web::get().to(hos_ws_route)))
            .app_data(web::Data::new(tx.clone()))
            .wrap(middleware::Logger::default())
    })
    .workers(2)
    .bind((listen_ip, listen_port))?
    .run()
    .await
}
