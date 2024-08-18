use std::{
    io::{self},
    net::SocketAddr,
    sync::Arc,
};

use client::Client;
use commands::handle_command;
use config::AdvancedConfiguration;
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
};


use config::BasicConfiguration;
use server::Server;

// Setup some tokens to allow us to identify which event is for which socket.

pub mod client;
pub mod commands;
pub mod config;
pub mod entity;
pub mod proxy;
// pub mod rcon;
pub mod server;
pub mod util;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[tokio::main]
async fn main() -> io::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    #[cfg(feature = "dhat-heap")]
    println!("Using a memory profiler");
    // ensure rayon is built outside of tokio scope
    rayon::ThreadPoolBuilder::new().build_global().unwrap();
    use std::time::Instant;

    // use rcon::RCONServer;

    let time = Instant::now();
    let basic_config = BasicConfiguration::load("configuration.toml");

    let advanced_configuration = AdvancedConfiguration::load("features.toml");

    simple_logger::SimpleLogger::new().init().unwrap();

    let addr: SocketAddr = format!(
        "{}:{}",
        basic_config.server_address, basic_config.server_port
    )
    .parse()
    .unwrap();

    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to start TCP Listener");

    let use_console = advanced_configuration.commands.use_console;
    let rcon = advanced_configuration.rcon.clone();

    let server = Arc::new(RwLock::new(Server::new((
        basic_config,
        advanced_configuration,
    ))));
    log::info!("Started Server took {}ms", time.elapsed().as_millis());
    log::info!("You now can connect to the server, Listening on {}", addr);

    if use_console {
        tokio::spawn(async move {
            let stdin = std::io::stdin();
            loop {
                let mut out = String::new();
                stdin
                    .read_line(&mut out)
                    .expect("Failed to read console line");
                handle_command(&mut commands::CommandSender::Console, &out).await;
            }
        });
    }
    // if rcon.enabled {
    //     tokio::spawn(async move {
    //         RCONServer::new(&rcon).await.unwrap();
    //     });
    // }
    let mut current_clients: u32 = 0;
    loop {
        let (socket, addr) = listener.accept().await?;
        log::info!("Accepted connection from: {}", addr);

        if let Err(e) = socket.set_nodelay(true) {
            log::error!("failed to set TCP_NODELAY: {e}");
        }
        let server = server.clone();

        current_clients += 1;
        let token = current_clients; // Replace with your token generation logic
        let client = Arc::new(Mutex::new(Client::new(token, socket, addr)));
        dbg!("a");
        let mut server_guard = server.write().await;
        server_guard.add_client(token, Arc::clone(&client));
        drop(server_guard);

        tokio::spawn(async move {
            let mut client = client.lock().await;

            //client.connection.readable().await.expect(":c");
            client.poll(server.clone()).await;
            if client.closed {
                let mut server_guard = server.write().await;
                server_guard.remove_client(&token).await;
                current_clients -= 1;
            }
        });
    }
}
