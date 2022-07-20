use log::{error, info};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::wrappers::UnboundedReceiverStream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures::stream::StreamExt;
use thiserror;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{accept_async, WebSocketStream};

use crate::crypto::PublicKey;
use crate::service::{Config, NetworkController, Publisher, Subscriber};
use crate::user::user::Address;

use super::client_info::ClientInfo;
use super::message::{ClientMessage, ServerMessage};
use super::subscription_router::Router;

#[derive(Clone)]
pub struct ApiServer {
    net: Arc<NetworkController>,
    publishers: Arc<Mutex<HashMap<Address, Publisher>>>,
    subscriber: Arc<Subscriber>,
    router: Arc<Mutex<Router>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiServerError {
    #[error("TCP socket error: {0}")]
    Tcp(io::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(tungstenite::error::Error),
    #[error("Sender error: {0}")]
    Sender(SendError<Message>),
}

impl ApiServer {
    pub async fn new(config: Config) -> ApiServer {
        let net = NetworkController::init(config).await;
        let publishers = Arc::new(Mutex::new(HashMap::new()));
        let subscriber = Arc::new(net.create_subscriber().await);
        let router = Arc::new(Mutex::new(Router::new(subscriber.clone())));

        ApiServer {
            net: Arc::new(net),
            publishers,
            subscriber,
            router,
        }
    }

    pub async fn start(self, bind_addr: String) -> Result<(), ApiServerError> {
        let listener = TcpListener::bind(bind_addr).await;

        if listener.is_err() {
            return Err(ApiServerError::Tcp(listener.err().unwrap()));
        }

        let listener = listener.ok().unwrap();

        {
            let mut router = self.router.lock().await;
            router.start();
        }

        let server = self.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        info!("TCP connection established: {}", addr);

                        match accept_async(socket).await {
                            Ok(websocket) => {
                                info!("WebSocket connection established: {}", addr);
                                let server = server.clone();
                                tokio::spawn(server.handle_connection(websocket, addr));
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

    async fn handle_connection(self, websocket: WebSocketStream<TcpStream>, addr: SocketAddr) {
        let (outgoing, mut incoming) = websocket.split();
        let (tx, rx) = unbounded_channel();

        let mut info = ClientInfo::new(tx);

        let rxstream = UnboundedReceiverStream::new(rx);

        let to_client = rxstream.map(|msg| Ok(msg)).forward(outgoing);

        let server = self.clone();

        let from_client = tokio::spawn(async move {
            while let Some(msg) = incoming.next().await {
                match msg {
                    Ok(msg) => match msg {
                        Message::Text(s) => {
                            if let Ok(msg) = serde_json::from_str::<ClientMessage>(&s) {
                                server.handle_client_message(&mut info, msg).await?;
                            } else {
                                continue;
                            }
                        }
                        Message::Ping(payload) => {
                            info.send(Message::Pong(payload))
                                .map_err(|e| ApiServerError::Sender(e))?;
                        }
                        Message::Close(cf) => {
                            info.send(Message::Close(cf))
                                .map_err(|e| ApiServerError::Sender(e))?;
                        }
                        _ => continue,
                    },
                    Err(e) => return Err(ApiServerError::WebSocket(e)),
                }
            }
            Ok(())
        });

        tokio::select! {
            _ = to_client => {}
            _ = from_client => {}
        }
    }

    async fn handle_client_message(
        &self,
        info: &mut ClientInfo,
        msg: ClientMessage,
    ) -> Result<(), ApiServerError> {
        match msg {
            ClientMessage::EstablishReq { addr, pubkey } => match PublicKey::from_bytes(&pubkey) {
                Ok(pubkey) => {
                    let addr = Address::new(addr);
                    let addr2 = Address::from(pubkey.clone());
                    if addr == addr2 {
                        let mut challenge = [0; 32];
                        ChaCha20Rng::from_entropy().fill_bytes(&mut challenge);
                        info.send_challenge(pubkey, challenge)
                            .map_err(|e| ApiServerError::Sender(e))?;
                    } else {
                        info.send_invalid().map_err(|e| ApiServerError::Sender(e))?;
                    }
                }
                Err(_) => {
                    info.send_invalid().map_err(|e| ApiServerError::Sender(e))?;
                }
            },
            ClientMessage::ChallengeResponce(sig) => {
                if let Ok(pk) = info.verify_challenge_sig(sig) {
                    info.send(Message::Text(
                        serde_json::to_string(&ServerMessage::Established).unwrap(),
                    ))
                    .map_err(|e| ApiServerError::Sender(e))?;

                    let mut publishers = self.publishers.lock().await;
                    publishers
                        .entry(Address::from(pk.clone()))
                        .or_insert(self.net.create_publisher(&pk).await);
                } else {
                    info.send_invalid().map_err(|e| ApiServerError::Sender(e))?;
                }
            }
            ClientMessage::SubscribeReq(addr) => {
                if info.is_established() {
                    let router = self.router.lock().await;
                    router.subscribe(addr, info.get_sender()).await;
                    info.send(Message::Text(
                        serde_json::to_string(&ServerMessage::Success).unwrap(),
                    ))
                    .map_err(|e| ApiServerError::Sender(e))?;
                } else {
                    info.send_invalid().map_err(|e| ApiServerError::Sender(e))?;
                }
            }
            ClientMessage::Post(post) => {
                if info.is_established() {
                    if let Some(pk) = info.get_pubkey(&post.addr) {
                        match post.verify(&pk) {
                            Ok(()) => {
                                let mut publishers = self.publishers.lock().await;
                                if let Some(publisher) =  publishers.get_mut(&post.addr) {
                                    publisher.publish(msg, dst)
                                }
                            }
                            Err(_) => {
                                info.send_invalid().map_err(|e|ApiServerError::Sender(e))?;
                            }
                        }
                    } else {
                        info.send(Message::Text(
                            serde_json::to_string(&ServerMessage::Denied).unwrap(),
                        ))
                        .map_err(|e| ApiServerError::Sender(e))?;
                    }
                } else {
                    info.send_invalid().map_err(|e|ApiServerError::Sender(e))?;
                }
            }
            _ => (),
        }
        Ok(())
    }
}
