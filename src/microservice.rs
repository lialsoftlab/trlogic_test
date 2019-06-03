use rouille;
use std::sync::mpsc;
use super::http_handlers;

pub fn init<'a>(host: &str, port: u16, upload_path: &str) -> (
    rouille::Server<impl Send + Sync + 'static + Fn(&rouille::Request) -> rouille::Response>,
    mpsc::Sender<&'static str>,
    mpsc::Receiver<&'static str>,
) {
    log::trace!("init...");

    let (srv_tx, srv_rx) = mpsc::channel::<&str>();
    let _ = {
        let srv_tx = srv_tx.clone();
        ctrlc::set_handler(move || {
            srv_tx
                .send("stop")
                .expect("Can't send stop signal to HTTP server!");
        })
    };

    log::debug!("Starting web server...");
    {
        let upload_path = String::from(upload_path);
        
        let server = match rouille::Server::new(format!("{}:{}", host, port), move |request| {
            http_handlers::route(&request, &upload_path)
        }) {
            Ok(x) => x,

            Err(e) => {
                log::error!("Can't start HTTP server! {}", e.to_string());
                panic!("Can't start HTTP server!");
            }
        };

        (server, srv_tx, srv_rx)
    }
}

pub fn run(
    server: rouille::Server<
        impl Send + Sync + 'static + Fn(&rouille::Request) -> rouille::Response,
    >,
    srv_rx: mpsc::Receiver<&'static str>,
) {
    log::info!("HTTP server listening...");

    loop {
        match srv_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(_) => break,
            _ => server.poll(),
        }
    }
}
