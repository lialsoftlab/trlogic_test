use pretty_env_logger;
use std::path::PathBuf;
use structopt::StructOpt;
use trlogic_test::microservice;

#[derive(Debug, StructOpt)]
#[structopt(name = "TRLogic test microservice", about = "A microservice for images upload.")]
struct Opt {
    /// Host address to bind microservice
    #[structopt(short, long, default_value = "localhost")]
    host: String,
    /// Port to listen for requests
    #[structopt(short, long, default_value = "8000")]
    port: u16,
    /// Upload path
    #[structopt(short, long, default_value="./uploads/", parse(from_os_str))]
    upload: PathBuf,
}

fn main() {
    pretty_env_logger::init_timed();
    log::trace!("main() setup...");

    let opt = Opt::from_args();

    if let Err(e) = std::fs::create_dir_all(&opt.upload) {
        log::error!("Can't use specified upload path! {}", e.to_string());
        panic!("Can't use specified upload path!");
    }

    let (server, _srv_tx, srv_rx) = microservice::init(&opt.host, opt.port, &opt.upload.to_string_lossy());
    microservice::run(server, srv_rx);

    log::trace!("main() shutdown.");
}
