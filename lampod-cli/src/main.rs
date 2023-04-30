#[allow(dead_code)]
mod args;

use std::env;
use std::io;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::JoinHandle;

use lampod::chain::{LampoWalletManager, WalletManager};
use log;

use lampo_common::conf::LampoConf;
use lampo_common::error;
use lampo_common::logger;
use lampo_jsonrpc::Handler;
use lampo_jsonrpc::JSONRPCv2;
use lampo_nakamoto::{Config, Nakamoto, Network};
use lampod::jsonrpc::inventory::get_info;
use lampod::jsonrpc::open_channel::json_open_channel;
use lampod::jsonrpc::peer_control::json_connect;
use lampod::jsonrpc::CommandHandler;
use lampod::LampoDeamon;

use crate::args::LampoCliArgs;

fn main() -> error::Result<()> {
    logger::init(log::Level::Info).expect("initializing logger for the first time");
    let args = args::parse_args()?;
    run(args)?;
    Ok(())
}

fn run(args: LampoCliArgs) -> error::Result<()> {
    let path = args.conf;
    let mut lampo_conf = LampoConf::try_from(path)?;

    lampo_conf.set_network(&args.network)?;

    let wallet = LampoWalletManager::new(lampo_conf.network)?;
    let mut lampod = LampoDeamon::new(lampo_conf.clone(), Arc::new(wallet));
    let client = match args.client.clone().as_str() {
        "nakamoto" => {
            let mut conf = Config::default();
            conf.network = Network::from_str(&lampo_conf.network.to_string()).unwrap();
            Arc::new(Nakamoto::new(conf).unwrap())
        }
        _ => error::bail!("client {:?} not supported", args.client),
    };
    lampod.init(client)?;

    let rpc_handler = Arc::new(CommandHandler::new(&lampo_conf)?);
    lampod.add_external_handler(rpc_handler.clone())?;

    let lampod = Arc::new(lampod);
    let (jsorpc_worker, handler) = run_jsonrpc(lampod.clone()).unwrap();
    rpc_handler.set_handler(handler.clone());
    lampod.listen()?;
    handler.stop();
    let _ = jsorpc_worker.join().unwrap();
    Ok(())
}

fn run_jsonrpc(
    lampod: Arc<LampoDeamon>,
) -> error::Result<(JoinHandle<io::Result<()>>, Arc<Handler<LampoDeamon>>)> {
    let socket_path = format!("{}/lampod.socket", lampod.root_path());
    env::set_var("LAMPO_UNIX", socket_path.clone());
    let server = JSONRPCv2::new(lampod, &socket_path)?;
    server.add_rpc("getinfo", get_info).unwrap();
    server.add_rpc("connect", json_connect).unwrap();
    server.add_rpc("fundchannel", json_open_channel).unwrap();
    let handler = server.handler();
    Ok((server.spawn(), handler))
}
