use actix_web::get;
use actix_web::{
    http::{header::ContentType, StatusCode},
    middleware, rt,
    web::{self, Data},
    App, Error, HttpRequest, HttpResponse, HttpServer,
};
use clap::{Arg, Command};
use hos_rv::state::AppState;
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
    if data.should_block == false {
        return true;
    }
    if let Some(val) = req.peer_addr() {
        val.ip().to_string() == data.allowed_ip
    } else {
        false
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
    let mut conns = data.hos_connections.write();
    if let Some(conn) = conns.get_mut(connection_id) {
        request_id = conn.lock().await.req("GET", &path).await.unwrap().clone();
        drop(conns);
    } else {
        return HttpResponse::build(StatusCode::NOT_FOUND).into();
    };

    for _ in 1..=10 {
        tokio::time::sleep(Duration::new(1, 0)).await;
        let conns = data.hos_connections.read();
        let conn = conns.get(connection_id).unwrap();
        for x in &conn.lock().await.incoming {
            if x.clone().id.unwrap_or("".to_string()) == request_id.to_string() {
                return HttpResponse::build(StatusCode::OK)
                    .content_type(ContentType::json())
                    .body(x.content.clone().unwrap());
            }
        }
        drop(conns);
    }

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
