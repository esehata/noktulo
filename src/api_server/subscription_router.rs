use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

use crate::service::Subscriber;
use crate::user::post::SignedPost;
use crate::user::user::Address;

use super::message::ServerMessage;

pub struct Router {
    routing_map: Arc<Mutex<HashMap<Address, Vec<UnboundedSender<Message>>>>>,
    subscriber: Arc<Subscriber>,
    is_started: bool,
}

impl Router {
    pub fn new(subscriber: Arc<Subscriber>) -> Router {
        Router {
            routing_map: Arc::new(Mutex::new(HashMap::new())),
            subscriber,
            is_started: false,
        }
    }

    pub fn start(&mut self) {
        // prevent a server from starting multiple times
        if self.is_started {
            return;
        }
        self.is_started = true;

        let mut rx = self.subscriber.get_receiver();

        let routing_map = self.routing_map.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        let mut routing_map = routing_map.lock().await;
                        match routing_map.get_mut(&msg.addr) {
                            Some(v) => {
                                let mut remove_list = Vec::new();
                                for (i, tx) in v.iter().enumerate() {
                                    if let Err(_) = tx.send(Message::Text(serde_json::to_string(&ServerMessage::Subscribed(msg.clone())).unwrap())) {
                                        remove_list.push(i);
                                    }
                                }
                                for i in remove_list.iter() {
                                    v.swap_remove(*i);
                                }
                            }
                            None => (),
                        };
                    }
                    Err(e) => {
                        match e {
                            RecvError::Closed => break,
                            RecvError::Lagged(_) => continue,
                        };
                    }
                }
            }
        });
    }

    pub async fn subscribe(&self, addr: Address, tx: UnboundedSender<Message>) {
        let mut routing_map = self.routing_map.lock().await;
        routing_map.entry(addr.clone()).or_insert(Vec::new()).push(tx);
        self.subscriber.subscribe(addr).await;
    }

    pub async fn unsubscribe(&self, addr: Address, tx: UnboundedSender<Message>) {
        let mut routing_map = self.routing_map.lock().await;
        if let Some(v) = routing_map.get_mut(&addr) {
            v.retain(|e| !e.same_channel(&tx));
            if v.is_empty() {
                routing_map.remove(&addr);
                self.subscriber.stop_subscription(&addr).await; 
            }
        }
    }
}
