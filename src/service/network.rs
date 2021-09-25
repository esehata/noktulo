use crate::account::user::Address;
use crate::crypto::PublicKey;
use crate::kad::Key;
use crate::kad::{Node, NodeInfo, Rpc};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::Mutex;

use super::{PUBSUB_DHT_KEY_LENGTH, TESTNET_PUBSUB_DHT, TESTNET_USER_DHT, USER_DHT_KEY_LENGTH};

pub struct UserDHT {
    rpc: Arc<Mutex<Rpc>>,
    user_dht: Arc<Node>,
    bootstrap: Option<NodeInfo>,
}

impl UserDHT {
    pub async fn start(port: u16, bootstrap: Option<NodeInfo>) -> UserDHT {
        let socket = UdpSocket::bind("0.0.0.0:".to_string() + &port.to_string())
            .await
            .unwrap();

        let rpc = Arc::new(Mutex::new(Rpc::new(socket)));

        // As of now, rx is not used
        let (tx, _rx) = mpsc::unbounded_channel();

        let user_dht = Node::start(
            TESTNET_USER_DHT.to_string(),
            USER_DHT_KEY_LENGTH,
            Key::random(USER_DHT_KEY_LENGTH),
            Arc::new(|data| UserDHT::is_valid_addr_pubkey_pair(data)),
            rpc.clone(),
            tx.clone(),
            bootstrap.clone(),
        )
        .await;

        UserDHT {
            rpc,
            user_dht: Arc::new(user_dht),
            bootstrap: bootstrap,
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

    pub async fn register_pubkey(&self, pubkey: &PublicKey) {
        let addr = Address::from_public_key(pubkey);
        let addr_key_pair = [&addr.to_bytes()[..], &pubkey.to_bytes()].concat();
        let key = Key::from_bytes(&addr.to_bytes());
        self.user_dht.put(key, &addr_key_pair).await;
    }
}

pub struct Publisher {
    node: Arc<Node>,
    rx: UnboundedReceiver<Vec<u8>>,
}

impl Publisher {
    pub async fn new(
        addr: Address,
        rpc: Arc<Mutex<Rpc>>,
        bootstrap: Option<NodeInfo>,
    ) -> Publisher {
        let mut id = Key::from_bytes(&addr.to_bytes());
        id.resize(PUBSUB_DHT_KEY_LENGTH);
        let (tx, rx) = mpsc::unbounded_channel();
        let node = Node::start(
            TESTNET_PUBSUB_DHT.to_string(),
            PUBSUB_DHT_KEY_LENGTH,
            id,
            Arc::new(|_| false),
            rpc,
            tx,
            bootstrap,
        )
        .await;

        Publisher {
            node: Arc::new(node),
            rx,
        }
    }

    pub async fn rx(&mut self) -> &mut UnboundedReceiver<Vec<u8>> {
        &mut self.rx
    }

    pub async fn publish(&self, msg: &[u8], dst: &Address) {
        let key = Key::from_bytes(&dst.to_bytes());
        self.node.multicast(&key, msg).await;
    }
}

pub struct Subscriber {
    rpc: Arc<Mutex<Rpc>>,
    nodes: Arc<Mutex<HashMap<Address, Node>>>,
    rx: UnboundedReceiver<Vec<u8>>,
    tx: UnboundedSender<Vec<u8>>,
    bootstrap: Option<NodeInfo>,
}

impl Subscriber {
    pub async fn new(rpc: Arc<Mutex<Rpc>>, bootstrap: Option<NodeInfo>) -> Subscriber {
        let (tx, rx) = mpsc::unbounded_channel();
        Subscriber {
            rpc,
            nodes: Arc::new(Mutex::new(HashMap::new())),
            rx,
            tx,
            bootstrap,
        }
    }

    pub async fn subscribe(&self, addr: Address) {
        let mut id = Key::from_bytes(&addr.to_bytes());
        id.resize_with_random(PUBSUB_DHT_KEY_LENGTH);
        let mut nodes = self.nodes.lock().await;
        if nodes.contains_key(&addr) {
            nodes.insert(
                addr,
                Node::start(
                    TESTNET_PUBSUB_DHT.to_string(),
                    PUBSUB_DHT_KEY_LENGTH,
                    id,
                    Arc::new(|_| false),
                    self.rpc.clone(),
                    self.tx.clone(),
                    self.bootstrap.clone(),
                )
                .await,
            );
        }
    }

    pub async fn rx(&mut self) -> &mut UnboundedReceiver<Vec<u8>> {
        &mut self.rx
    }

    pub async fn stop_subscription(&self, addr: &Address) {
        let mut nodes = self.nodes.lock().await;
        nodes.remove(addr);
    }
}
