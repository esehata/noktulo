use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use super::key::Key;
use super::node::{Reply, Request};
use super::routing::NodeInfo;

use super::{KEY_LEN, MESSAGE_LEN, TIME_OUT};

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
            src: src,
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
        let is_start = self.is_start.lock().await;
        if !(*is_start) {
            let rpc = self.clone();
            tokio::spawn(async move {
                let mut buf = [0; MESSAGE_LEN];
                loop {
                    let (len, src_addr) = rpc.socket.recv_from(&mut buf).await.unwrap();
                    let mut rmsg: RpcMessage;
                    match serde_json::from_str(str::from_utf8(&buf[..len]).unwrap()) {
                        Ok(e) => rmsg = e,
                        Err(_) => {
                            if cfg!(debug_assertions) {
                                println!("WARN: Message with invalid json, ignoring.");
                            }
                            continue;
                        }
                    };
                    rmsg.src.addr = src_addr;

                    if cfg!(debug_assertions) {
                        println!(
                            "DEBUG: |  IN | {:?} {:?} <== {:?}",
                            rmsg.token, rmsg.msg, rmsg.src.id
                        );
                    }

                    let node_infos = rpc.node_infos.lock().await;
                    let node_info = node_infos.iter().find(|x| x.0.id == rmsg.dst.id);

                    match node_info {
                        Some(node_info) => {
                            if rmsg.src.net_id != node_info.0.net_id {
                                if cfg!(debug_assertions) {
                                    println!(
                                        "WARN: Message from different net_id received, ignoring."
                                    );
                                }
                                continue;
                            }

                            match rmsg.msg {
                                Message::Kill => {
                                    break;
                                }
                                Message::Request(req) => {
                                    let req_handle = ReqHandle {
                                        token: rmsg.token,
                                        src: rmsg.src,
                                        req: req,
                                        rpc: rpc.clone(),
                                    };
                                    if let Err(_) = node_info.1.send(req_handle) {
                                        if cfg!(debug_assertions) {
                                            println!(
                                                "INFO: Closing channel, since receiver is dead."
                                            );
                                        }
                                        break;
                                    }
                                }
                                Message::Reply(rep) => {
                                    rpc.clone().handle_rep(rmsg.token, rep).await;
                                }
                            }
                        }
                        None => {
                            if cfg!(debug_assertions) {
                                println!("WARN: Message received, but dst id does not match any nodes, ignoring.");
                            }
                            continue;
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
        tx: UnboundedSender<ReqHandle>,
        node_info: NodeInfo,
    ) -> Rpc {
        let mut rpc = Rpc::new(socket);
        rpc.add(tx, node_info).await;

        let ret = rpc.clone();
        rpc.start_server().await;

        ret
    }

    pub async fn add(&mut self, tx: UnboundedSender<ReqHandle>, node_info: NodeInfo) {
        let mut node_infos = self.node_infos.lock().await;
        node_infos.push((node_info, tx));
    }

    async fn handle_rep(self, token: Key, rep: Reply) {
        tokio::spawn(async move {
            let mut pending = self.pending.lock().await;
            let send_res = match pending.get(&token) {
                Some(tx) => {
                    if cfg!(debug_assertions) {
                        println!("INFO: Reply received: {:?}", token);
                    }
                    tx.send(Some(rep))
                }
                None => {
                    if cfg!(debug_assertions) {
                        println!("WARN: Unsolicited reply received, ignoring: {:?}", token);
                    }
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
        if cfg!(debug_assertions) {
            println!(
                "DEBUG: | OUT | {:?} {:?} ==> {:?} ",
                rmsg.token, rmsg.msg, rmsg.dst.id
            );
        }
    }

    pub async fn send_req(
        &self,
        req: Request,
        src: NodeInfo,
        dst: NodeInfo,
    ) -> UnboundedReceiver<Option<Reply>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut pending = self.pending.lock().await;
        let mut token = Key::random(KEY_LEN);
        while pending.contains_key(&token) {
            token = Key::random(KEY_LEN);
        }
        pending.insert(token.clone(), tx.clone());
        drop(pending);

        let node_infos = self.node_infos.lock().await;
        if let None = node_infos.iter().find(|x| x.0 == src) {
            panic!("Invalid source node!");
        }
        drop(node_infos);

        let rmsg = RpcMessage {
            token: token.clone(),
            src: src,
            dst: dst,
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
                    if cfg!(debug_assertions) {
                        println!("INFO: Removed pending token: {:?}", token);
                    }
                };
            }
        });
        rx
    }
}
