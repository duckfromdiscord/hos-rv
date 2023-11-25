use actix_web::{
    middleware, rt, web::{self, Data}, App, Error, HttpRequest, HttpResponse, HttpServer, http::{StatusCode, header::ContentType}
};
use clap::{Arg, Command};
use crossbeam::channel::Receiver;
use hos_rv::{ws_handler, json::HOSConnectionList};
use tokio::sync::broadcast;
use actix_web::get;
use std::{collections::HashMap, time::Duration};
use hos_rv::state::AppState;
use base64::{Engine as _, engine::general_purpose};

const DEFAULT_TIMEOUT_SECS: Duration = Duration::new(3,0);

async fn hos_ws_route(req: HttpRequest, stream: web::Payload, data: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;
    rt::spawn(ws_handler::hos_ws(session, msg_stream, data));

    Ok(res)
}

fn is_allowed(req: HttpRequest, data: Data<AppState>) -> bool {
    if data.should_block == false {
        return true;
    }
    if let Some(val) = req.peer_addr() {
        val.ip().to_string() == "127.0.0.1"
    } else {
        false
    }
}

fn full_query_path(req: HttpRequest, path: &str) -> String {
    match req.query_string().is_empty() {
        true => {
            path.to_string()
        },
        false => {
            path.to_string() + "?" + req.query_string()
        }
    }
}

fn await_hos_recv(recv: Receiver<String>, timeout: Duration) -> Option<Vec<u8>> {
    match recv.recv_timeout(timeout) {
        Ok(received_data) => {
            let b64 = received_data.to_string();
            let bytes = general_purpose::STANDARD.decode(b64).unwrap();
            Some(bytes)
        },
        Err(_) => {
            None
        }
    }
    
}

#[get("/list")]
pub async fn handle_list_req(
    req: HttpRequest,
    data: Data<AppState>,
) -> HttpResponse {

    if !(is_allowed(req.clone(), data.clone())) {
        return HttpResponse::build(StatusCode::UNAUTHORIZED).into();
    }

    let mut connections: Vec<(String, String)> = vec![];

    let mut conns = data.hos_connections.lock().await;
    for conn in conns.values_mut() {
        connections.push((conn.connection_id.to_string(), conn.pairing_code.clone().unwrap_or("".to_string())));
    }


    let connection_list = HOSConnectionList {
        connections,
    };

    HttpResponse::build(StatusCode::OK).content_type(ContentType::json()).body(
        serde_json::to_string(&connection_list).unwrap()
    )
}

#[get("pid/{pairing_id}/{tail:.*}")]
pub async fn handle_pid_get(
    req: HttpRequest,
    info: web::Path<(String, String)>,
    data: Data<AppState>
) -> HttpResponse {

    if !(is_allowed(req.clone(), data.clone())) {
        return HttpResponse::build(StatusCode::UNAUTHORIZED).into();
    }

    let pairing_id = &info.0;
    let path = full_query_path(req, &info.1);
    
    let mut conns = data.hos_connections.lock().await;
    for conn in conns.values_mut() {
        if conn.pairing_code.clone().unwrap_or("".to_string()) == *pairing_id {
                println!("a");
                let recv = conn.req("GET", &path.clone()).await.unwrap().clone();
                println!("b");
                drop(conns);
                println!("c");
                let bytes = await_hos_recv(recv, DEFAULT_TIMEOUT_SECS).unwrap();
                println!("d");
                return HttpResponse::build(StatusCode::OK).content_type(ContentType::json()).body(bytes);
        } else {
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).into();
        }
    }
    HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).into()
}

#[get("sid/{session_id}/{tail:.*}")]
pub async fn handle_sid_get(
    req: HttpRequest,
    info: web::Path<(String, String)>,
    data: Data<AppState>
) -> HttpResponse {

    if !(is_allowed(req.clone(), data.clone())) {
        return HttpResponse::build(StatusCode::UNAUTHORIZED).into();
    }

    let connection_id = &info.0;
    let path = full_query_path(req, &info.1);
    
    let mut conns = data.hos_connections.lock().await;
    if let Some(conn) = conns.get_mut(connection_id) {
        let recv = conn.req("GET", &path).await.unwrap().clone();
        drop(conns);
        let bytes = await_hos_recv(recv, DEFAULT_TIMEOUT_SECS).unwrap();
        return HttpResponse::build(StatusCode::OK).content_type(ContentType::json()).body(bytes);
    } else {
        return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).into();
    }
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
            .help("Only allow requests from 127.0.0.1")
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
    
    let should_block: bool = matches
        .get_flag("block");

    log::info!(
        "starting HTTP server at http://{}:{}",
        listen_ip,
        listen_port
    );

    if should_block {
        log::warn!("Blocking all non-127.0.0.1 requests");
    } else {
        log::warn!("Responding to requests from any address. Remember to run with `-b` to block requests that don't come from localhost.");
    }

    let (tx, _) = broadcast::channel::<web::Bytes>(128);

    let state = web::Data::new(AppState {
        hos_connections: HashMap::new().into(),
        should_block,
    });


    HttpServer::new(move || {
        App::new()
        .app_data(state.clone())
            .service(handle_list_req)
            .service(handle_pid_get)
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
