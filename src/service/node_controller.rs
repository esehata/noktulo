use crate::account::user::Address;
use crate::crypto::PublicKey;
use crate::kad::Key;
use crate::kad::{Node, NodeInfo, Rpc};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

pub struct NodeController {
    rpc: Arc<Mutex<Rpc>>,
    user_dht: Arc<Node>,
    publish_nodes: Arc<Mutex<HashMap<Key, Node>>>,
    bootstrap: Option<NodeInfo>,
}

impl NodeController {
    pub async fn start(port: u16, bootstrap: Option<NodeInfo>) -> NodeController {
        let socket = UdpSocket::bind("0.0.0.0:".to_string() + &port.to_string())
            .await
            .unwrap();

        let rpc = Arc::new(Mutex::new(Rpc::new(socket)));

        let (tx, _rx) = mpsc::unbounded_channel();

        let user_dht = Node::start(
            "test_net".to_string(),
            32,
            Key::random(32),
            Arc::new(|data| NodeController::is_valid_addr_pubkey_pair(data)),
            rpc.clone(),
            tx.clone(),
            bootstrap,
        )
        .await;

        NodeController {
            rpc,
            user_dht: Arc::new(user_dht),
            publish_nodes: Arc::new(Mutex::new(Vec::new())),
            bootstrap: None,
        }
    }

    pub fn is_valid_addr_pubkey_pair(data: &[u8]) -> bool {
        if data.len() != 65 {
            false
        } else {
            let addr_bytes = &data[..33];
            let addr = Address::from_bytes(addr_bytes.try_into().unwrap());
            let pk = PublicKey::from_bytes(&data[33..].try_into().unwrap());
            let addr2 = Address::from_public_key(&pk);
            addr == addr2
        }
    }

    

    /* pub async fn subscribe(
        &self,
        addr: Address,
        tx: UnboundedSender<Vec<u8>>,
        bootstrap: Option<NodeInfo>,
    ) {
        let mut id = Key::from_bytes(&addr.to_bytes());
        id.resize_with_random(64);
        Node::start(
            "test_net".to_string(),
            64,
            id,
            Arc::new(|_| true),
            self.rpc.clone(),
            tx,
            bootstrap,
        )
        .await;
    } */

    /* pub async fn publish(&self, sender_addr: Address, receiver_addr: Address, msg: &[u8]) {
        let mut id = Key::from_bytes(&sender_addr.to_bytes());
        id.resize(64);
        let publish_nodes = self.publish_nodes.lock().await;
        if publish_nodes.contains_key(&id) {
            publish_nodes.entry(id).or_insert(Node::start(
                "test_net".to_string(),
                64,
                id,
                Arc::new(|_| true),
                self.rpc.clone(),
                ,
                bootstrap,
            ))
        }
    } */
}
