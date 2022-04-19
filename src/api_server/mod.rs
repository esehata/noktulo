mod message;

use log::{error, info};
pub use message::ClientMessage;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{accept_async, WebSocketStream};

use crate::service::{Config, NetworkController, Publisher, Subscriber};
use crate::user::post::SignedPost;
use crate::user::user::Address;

use self::message::ServerMessage;

#[derive(Debug, Serialize, Deserialize)]
pub enum ApiMessageKind {
    Send(SignedPost),
    Subscribe(Vec<Address>),
    Unsubscribe(Vec<Address>),
}
pub struct ApiServer {
    net: NetworkController,
    publishers: HashMap<Address, Publisher>,
    subscriber: Subscriber,
    peermap: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<ServerMessage>>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiServerError {
    #[error("Tcp socket error: {0}")]
    Tcp(io::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(tungstenite::error::Error),
}

impl ApiServer {
    pub async fn new(config: Config) -> ApiServer {
        let net = NetworkController::init(config).await;
        let publishers = HashMap::new();
        let subscriber = net.create_subscriber().await;

        ApiServer {
            net,
            publishers,
            subscriber,
            peermap: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self, bind_addr: String) -> Result<(), ApiServerError> {
        let listener = TcpListener::bind(bind_addr).await;

        if listener.is_err() {
            return Err(ApiServerError::Tcp(listener.err().unwrap()));
        }

        let listener = listener.ok().unwrap();
        let peermap = self.peermap.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        info!("TCP connection established: {}", addr);

                        match accept_async(socket).await {
                            Ok(websocket) => {
                                info!("WebSocket connection established: {}", addr);
                                let peermap = peermap.clone();
                                tokio::spawn(ApiServer::handle_connection(
                                    websocket, addr, peermap,
                                ));
                            }
                            Err(e) => {
                                error!("WebSocket error occured on {}: {}", addr, e);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        error!("TCP connection error occured on: {}", e);
                        return ApiServerError::Tcp(e);
                    }
                }
            }
        });

        Ok(())
    }

    async fn handle_connection(
        websocket: WebSocketStream<TcpStream>,
        addr: SocketAddr,
        peermap: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<ServerMessage>>>>,
    ) {
        let (outgoing, incoming) = websocket.split();
        let (tx, rx) = unbounded_channel();

        let mut pm = peermap.lock().await;
        pm.insert(addr, tx);

        let rxstream = UnboundedReceiverStream::new(rx);

        let to_client = rxstream
            .map(|msg| Ok(Message::Text(serde_json::to_string(&msg).unwrap())))
            .forward(outgoing);

        let from_client = incoming.map(|msg| match msg {
            Ok(msg) => match msg {
                Message::Text(s) => if let Ok(msg) = serde_json::from_str::<ClientMessage>(&s) {},
                _ => (),
            },
            Err(_) => (),
        });
    }
}
