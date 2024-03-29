use crate::crypto::PublicKey;
use crate::kad::Key;
use crate::kad::{Node, NodeInfo, Rpc};
use crate::user::post::SignedPost;
use crate::user::user::Address;
use log::info;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::{broadcast, Mutex};

use super::{PUBSUB_DHT_KEY_LENGTH, TESTNET_PUBSUB_DHT, TESTNET_USER_DHT, USER_DHT_KEY_LENGTH};

pub struct UserDHT {
    user_dht: Arc<Node>,
}

impl UserDHT {
    pub async fn start(rpc: Arc<Mutex<Rpc>>, bootstrap: &[NodeInfo]) -> UserDHT {
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
        info!("User DHT node started");

        UserDHT {
            user_dht: Arc::new(user_dht),
        }
    }

    pub fn is_valid_addr_pubkey_pair(data: &[u8]) -> bool {
        if data.len() != 64 {
            false
        } else {
            let addr = Address::new(data[..32].try_into().unwrap());
            if let Ok(pk) = PublicKey::from_bytes(&data[32..].try_into().unwrap()) {
                let addr2 = Address::from(pk);
                addr == addr2
            } else {
                false
            }
        }
    }

    pub async fn register_pubkey(&self, pubkey: &PublicKey) {
        let addr_bytes: [u8; 32] = Address::from(pubkey.clone()).into();
        let pk_bytes: [u8; 32] = pubkey.clone().into();
        let addr_key_pair = [&addr_bytes[..], &pk_bytes].concat();
        let key = Key::from(&addr_bytes[..]);
        self.user_dht.put(key, &addr_key_pair).await;
    }

    pub async fn get_pubkey(&self, addr: Address) -> Option<PublicKey> {
        let key = Key::from(addr);
        if let Some(bytes) = self.user_dht.get(key).await {
            if UserDHT::is_valid_addr_pubkey_pair(&bytes) {
                return Some(PublicKey::from_bytes(&bytes[32..].try_into().unwrap()).unwrap());
            }
        }

        None
    }
}

pub struct Publisher {
    node: Arc<Node>,
    rx: UnboundedReceiver<Vec<u8>>,
}

impl Publisher {
    pub async fn new(addr: Address, rpc: Arc<Mutex<Rpc>>, bootstrap: &[NodeInfo]) -> Publisher {
        let mut id: Key = addr.into();
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
        let key = Key::from(dst.clone());
        self.node.multicast(&key, msg).await;
        info!("Hoot multicast");
    }
}

pub struct Subscriber {
    rpc: Arc<Mutex<Rpc>>,
    nodes: Arc<Mutex<HashMap<Address, Node>>>,
    tx: UnboundedSender<Vec<u8>>,
    broadcast_tx: broadcast::Sender<SignedPost>,
    broadcast_rx: broadcast::Receiver<SignedPost>,
    bootstrap: Vec<NodeInfo>,
}

impl Subscriber {
    pub async fn new(rpc: Arc<Mutex<Rpc>>, bootstrap: &[NodeInfo]) -> Subscriber {
        let (bc_tx, bc_rx) = broadcast::channel(16);
        let bc_tx2 = bc_tx.clone();

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Ok(post) = SignedPost::from_bytes(&msg) {
                    bc_tx2.send(post).unwrap();
                }
            }
        });

        Subscriber {
            rpc,
            nodes: Arc::new(Mutex::new(HashMap::new())),
            tx,
            broadcast_tx: bc_tx,
            broadcast_rx: bc_rx,
            bootstrap: bootstrap.to_vec(),
        }
    }

    pub async fn subscribe(&self, addr: Address) {
        let mut id = Key::from(addr.clone());
        id.resize_with_random(PUBSUB_DHT_KEY_LENGTH);
        let mut nodes = self.nodes.lock().await;
        if !nodes.contains_key(&addr) {
            nodes.insert(
                addr,
                Node::start(
                    TESTNET_PUBSUB_DHT.to_string(),
                    PUBSUB_DHT_KEY_LENGTH,
                    id,
                    Arc::new(|_| false),
                    self.rpc.clone(),
                    self.tx.clone(),
                    &self.bootstrap,
                )
                .await,
            );
        }
    }

    pub fn get_receiver(&self) -> broadcast::Receiver<SignedPost> {
        self.broadcast_tx.subscribe()
    }

    pub async fn stop_subscription(&self, addr: &Address) {
        let mut nodes = self.nodes.lock().await;
        nodes.remove(addr);
    }
}
