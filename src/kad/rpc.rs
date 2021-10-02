use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::str;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use super::key::Key;
use super::node::{Reply, Request};
use super::routing::NodeInfo;

use super::{MESSAGE_LEN, TIME_OUT, TOKEN_KEY_LEN};
use crate::service::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcMessage {
    token: Key,
    src: NodeInfo,
    dst: NodeInfo,
    msg: Message,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    Kill,
    Request(Request),
    Reply(Reply),
}

pub struct ReqHandle {
    token: Key,
    src: NodeInfo,
    req: Request,
    rpc: Rpc,
}

impl ReqHandle {
    pub fn get_req(&self) -> &Request {
        &self.req
    }

    pub fn get_src(&self) -> &NodeInfo {
        &self.src
    }

    pub async fn rep(self, rep: Reply, src: NodeInfo) {
        let rep_rmsg = RpcMessage {
            token: self.token,
            src,
            dst: self.src.clone(),
            msg: Message::Reply(rep),
        };
        self.rpc.send_msg(&rep_rmsg, self.src.addr).await;
    }
}

#[derive(Clone)]
pub struct Rpc {
    pub socket: Arc<UdpSocket>,
    is_start: Arc<Mutex<bool>>,
    pending: Arc<Mutex<HashMap<Key, UnboundedSender<Option<Reply>>>>>,
    node_infos: Arc<Mutex<Vec<(NodeInfo, UnboundedSender<ReqHandle>)>>>,
}

impl Rpc {
    pub fn new(socket: UdpSocket) -> Rpc {
        Rpc {
            socket: Arc::new(socket),
            is_start: Arc::new(Mutex::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            node_infos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn start_server(&self) {
        let mut is_start = self.is_start.lock().await;
        if !(*is_start) {
            *is_start = true;
            let rpc = self.clone();
            tokio::spawn(async move {
                loop {
                    let mut buf = [0; MESSAGE_LEN];
                    let (len, src_addr) = rpc.socket.recv_from(&mut buf).await.unwrap();
                    let mut rmsg: RpcMessage;
                    match serde_json::from_str(str::from_utf8(&buf[..len]).unwrap()) {
                        Ok(e) => rmsg = e,
                        Err(_) => {
                            warn!("Message with invalid json, ignoring.");
                            continue;
                        }
                    };
                    rmsg.src.addr = src_addr;

                    debug!(
                        "|  IN | {:?} {:?} <== {:?}",
                        rmsg.token, rmsg.msg, rmsg.src.id
                    );

                    let mut node_infos = rpc.node_infos.lock().await;
                    let node_info = node_infos
                        .iter()
                        .enumerate()
                        .find(|(_, x)| x.0.id == rmsg.dst.id);

                    match node_info {
                        Some((index, node_info)) => {
                            if rmsg.src.net_id != node_info.0.net_id {
                                warn!("Message from different net_id received, ignoring.");
                                continue;
                            }

                            match rmsg.msg {
                                Message::Kill => {
                                    //break;
                                }
                                Message::Request(req) => {
                                    let req_handle = ReqHandle {
                                        token: rmsg.token,
                                        src: rmsg.src,
                                        req,
                                        rpc: rpc.clone(),
                                    };
                                    if let Err(_) = node_info.1.send(req_handle) {
                                        info!("Closing channel, since receiver is dead.");
                                        node_infos.swap_remove(index);
                                    }
                                }
                                Message::Reply(rep) => {
                                    rpc.clone().handle_rep(rmsg.token, rep).await;
                                }
                            }
                        }
                        None => {
                            warn!(
                                "Message received, but dst id does not match any nodes, ignoring."
                            );
                            if node_infos.is_empty() {
                                break;
                            } else {
                                continue;
                            }
                        }
                    }

                    drop(node_infos);
                }
            });
        }
        drop(is_start);
    }

    pub async fn open(
        socket: UdpSocket,
        node_info: NodeInfo,
        tx: UnboundedSender<ReqHandle>,
    ) -> Rpc {
        let mut rpc = Rpc::new(socket);
        rpc.add(node_info, tx).await;

        let ret = rpc.clone();
        rpc.start_server().await;

        ret
    }

    pub async fn add(&mut self, node_info: NodeInfo, tx: UnboundedSender<ReqHandle>) {
        let mut node_infos = self.node_infos.lock().await;
        node_infos.push((node_info, tx.clone()));
        drop(node_infos);
    }

    async fn handle_rep(self, token: Key, rep: Reply) {
        tokio::spawn(async move {
            let mut pending = self.pending.lock().await;
            let send_res = match pending.get(&token) {
                Some(tx) => {
                    info!("Reply received: {:?}", token);
                    tx.send(Some(rep))
                }
                None => {
                    warn!("Unsolicited reply received, ignoring: {:?}", token);
                    return;
                }
            };
            if let Ok(_) = send_res {
                pending.remove(&token);
            }
        });
    }

    async fn send_msg(&self, rmsg: &RpcMessage, addr: SocketAddr) {
        let enc_msg = serde_json::to_string(rmsg).unwrap();
        self.socket
            .send_to(&enc_msg.as_bytes(), addr)
            .await
            .unwrap();
        debug!(
            "| OUT | {:?} {:?} ==> {:?} ",
            rmsg.token, rmsg.msg, rmsg.dst.id
        );
    }

    pub async fn send_req(
        &self,
        req: Request,
        src: NodeInfo,
        dst: NodeInfo,
    ) -> UnboundedReceiver<Option<Reply>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut pending = self.pending.lock().await;
        let mut token = Key::random(TOKEN_KEY_LEN);
        while pending.contains_key(&token) {
            token = Key::random(TOKEN_KEY_LEN);
        }
        pending.insert(token.clone(), tx.clone());
        drop(pending);

        let node_infos = self.node_infos.lock().await;
        if let None = node_infos.iter().find(|(x, _)| *x == src) {
            panic!("Invalid source node!");
        }
        drop(node_infos);

        let rmsg = RpcMessage {
            token: token.clone(),
            src,
            dst,
            msg: Message::Request(req),
        };
        self.send_msg(&rmsg, rmsg.dst.addr).await;

        let rpc = self.clone();
        let token = token.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(TIME_OUT)).await;
            if let Ok(_) = tx.send(None) {
                let mut pending = rpc.pending.lock().await;
                if let Some(_) = pending.remove(&token) {
                    info!("Removed pending token: {:?}", token);
                };
            }
        });
        rx
    }

    async fn node_infos(&self) -> Vec<NodeInfo> {
        let node_infos = self.node_infos.lock().await;
        node_infos.iter().map(|(ni, _)| ni.clone()).collect()
    }

    pub async fn start_nodeinfo_server(&self, addr: SocketAddr) -> io::Result<()> {
        let rpc = self.clone();
        let listener = TcpListener::bind(addr).await?;
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let rpc = rpc.clone();
                tokio::spawn(async move {
                    let mut stream = BufReader::new(socket);
                    let mut first_line = String::new();
                    stream.read_line(&mut first_line).await.unwrap();

                    let mut params = first_line.split_whitespace();
                    let method = params.next();
                    let query = params.next();

                    match (method, query) {
                        (Some("GET"), Some(query)) => {
                            let mut node_infos = rpc.node_infos().await;
                            match query {
                                "test" => {
                                    node_infos = node_infos
                                        .iter()
                                        .filter(|x| {
                                            x.net_id == TESTNET_USER_DHT
                                                || x.net_id == TESTNET_PUBSUB_DHT
                                        })
                                        .cloned()
                                        .collect();
                                }
                                "main" => {
                                    node_infos = node_infos
                                        .iter()
                                        .filter(|x| {
                                            x.net_id == MAINNET_USER_DHT
                                                || x.net_id == MAINNET_PUBSUB_DHT
                                        })
                                        .cloned()
                                        .collect();
                                }
                                _ => (),
                            }
                            let msg = serde_json::to_string(&node_infos).unwrap();
                            stream
                                .get_mut()
                                .write_all(
                                    format!(
                                        "HTTP/1.1 200 OK\r\n
                                    Content-Type: application/json; charset=UTF-8\r\n
                                    Content-Length: {}\r\n\r\n{}",
                                        msg.len(),
                                        msg
                                    )
                                    .as_bytes(),
                                )
                                .await
                                .unwrap();
                        }
                        _ => {
                            stream
                                .get_mut()
                                .write_all("HTTP/1.1 400 Bad Request\r\n\r\n".as_bytes())
                                .await
                                .unwrap();
                        }
                    }
                });
            }
        });

        Ok(())
    }

    pub async fn get_nodeinfos(addr: SocketAddr) -> io::Result<Vec<NodeInfo>> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.write_all("GET test".as_bytes()).await?;

        let mut buf = String::new();
        let mut stream = BufReader::new(stream);
        stream.read_line(&mut buf).await?; // HTTP/1.1 200 OK\r\n
        stream.read_line(&mut buf).await?; // Content-Type: application/json; charset=UTF-8\r\n
        stream.read_line(&mut buf).await?; // Content-Length: {}\r\n
        stream.read_line(&mut buf).await?; // \r\n
        stream.read_to_string(&mut buf).await?; // Content

        let node_infos = serde_json::from_str(&buf)?;

        Ok(node_infos)
    }
}
