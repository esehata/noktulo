use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;

use crate::kad::KEY_LEN;

use super::key::Key;
use super::routing::{NodeInfo, RoutingTable};
use super::rpc::{ReqHandle, Rpc};
use super::{A_PARAM, K_PARAM};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Store(Key, Vec<u8>),
    FindNode(Key),
    FindValue(Key),
    Unicast(Vec<u8>),
    Broadcast(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FindValueResult {
    Nodes(Vec<(NodeInfo, Key)>),
    Value(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Reply {
    Ping,
    FindNode(Vec<(NodeInfo, Key)>),
    FindValue(FindValueResult),
}

#[derive(Clone)]
pub struct Node {
    key_length: usize,
    routes: Arc<Mutex<RoutingTable>>,
    store: Arc<Mutex<HashMap<Key, Vec<u8>>>>,
    store_predicate: Arc<dyn Fn(&[u8]) -> bool + Sync + Send>,
    broadcast_tokens: Arc<Mutex<HashSet<Key>>>,
    rpc: Arc<Mutex<Rpc>>,
    node_info: NodeInfo,
}

impl Node {
    pub async fn start(
        net_id: String,
        key_length: usize,
        node_id: Key,
        store_requirement: Arc<dyn Fn(&[u8]) -> bool + Sync + Send>,
        rpc: Arc<Mutex<Rpc>>,
        bootstrap: Option<NodeInfo>,
    ) -> Node {
        assert_eq!(key_length, node_id.len());
        let (tx, rx) = mpsc::unbounded_channel();
        let mut rpc_raw = rpc.lock().await;
        let socket = rpc_raw.socket.clone();

        let node_info = NodeInfo {
            id: node_id.clone(),
            addr: socket.local_addr().unwrap(),
            net_id: net_id,
        };

        rpc_raw.add(tx.clone(), node_info.clone()).await;
        rpc_raw.start_server().await;
        drop(rpc_raw);

        let mut routes = RoutingTable::new(key_length, &node_info.clone());
        if let Some(bootstrap) = bootstrap {
            routes.update(bootstrap);
        }

        if cfg!(debug_assertions) {
            println!(
                "INFO: new node created at {} with ID {:?}",
                &node_info.addr, &node_info.id
            );
        }

        let node = Node {
            key_length,
            routes: Arc::new(Mutex::new(routes)),
            store: Arc::new(Mutex::new(HashMap::new())),
            store_predicate: store_requirement,
            broadcast_tokens: Arc::new(Mutex::new(HashSet::new())),
            rpc: rpc.clone(),
            node_info,
        };

        node.clone().start_req_handler(rx).await;

        node.lookup_nodes(node_id).await;

        node
    }

    pub async fn start_req_handler(self, mut rx: UnboundedReceiver<ReqHandle>) {
        tokio::spawn(async move {
            while let Some(req_handle) = rx.recv().await {
                let node = self.clone();
                tokio::spawn(async move {
                    let rep =
                        node.handle_req(req_handle.get_req().clone(), req_handle.get_src().clone());
                    req_handle.rep(rep.await, node.node_info.clone()).await;
                });
            }
            if cfg!(debug_assertions) {
                println!("INFO: Channnel closed, since sender is dead.");
            }
        });
    }

    pub async fn handle_req(&self, req: Request, src: NodeInfo) -> Reply {
        let mut routes = self.routes.lock().await;
        // update routes
        if let Some(e) = routes.update(src.clone()) {
            let node = self.clone();
            tokio::spawn(async move {
                let mut routes = node.routes.lock().await;
                // ping the old node and re-update routes
                if let None = node.ping(e.clone()).await {
                    routes.remove(&e);
                    routes.update(src);
                }
                drop(routes);
            });
        }
        drop(routes);

        let ret = match req {
            Request::Ping => Reply::Ping,
            Request::Store(k, v) => {
                if self.key_length != k.len() {
                    println!("INFO: Store request which has invalid key length, ignoring.");
                } else {
                    let mut store = self.store.lock().await;
                    // check whether the value is valid
                    if (self.store_predicate)(&v) {
                        store.insert(k, v);
                    }
                }
                Reply::Ping
            }
            Request::FindNode(id) => {
                if self.key_length != id.len() {
                    println!("INFO: FindNode request which has invalid key length, ignoring.");
                    Reply::FindNode(Vec::new())
                } else {
                    let routes = self.routes.lock().await;
                    Reply::FindNode(routes.closest_nodes(id, K_PARAM))
                }
            }
            Request::FindValue(k) => {
                if self.key_length != k.len() {
                    println!("INFO: FindValue request which has invalid key length, ignoring.");
                    return Reply::FindValue(FindValueResult::Nodes(Vec::new()));
                }

                let hash = k.to_hash();

                let store = self.store.lock().await;
                let lookup_res = store.get(&k);
                let ret = match lookup_res {
                    Some(v) => Reply::FindValue(FindValueResult::Value(v.to_vec())),
                    None => {
                        let routes = self.routes.lock().await;
                        Reply::FindValue(FindValueResult::Nodes(
                            routes.closest_nodes(hash, K_PARAM),
                        ))
                    }
                };

                drop(store);

                ret
            }
            Request::Unicast(msg) => {
                println!(
                    "{}",
                    String::from_utf8(msg).unwrap_or(String::from("INVALID MESSAGE"))
                );

                Reply::Ping
            }
            Request::Broadcast(msg) => {
                println!(
                    "{}",
                    String::from_utf8(msg.clone()).unwrap_or(String::from("INVALID MESSAGE"))
                );
                let broadcast_tokens = self.broadcast_tokens.lock().await;
                let is_relay = !broadcast_tokens.contains(&Key::hash(&msg, KEY_LEN));

                drop(broadcast_tokens);

                if is_relay {
                    let node = self.clone();
                    tokio::spawn(async move { node.broadcast(&msg).await });
                } else {
                    if cfg!(debug_assertions) {
                        println!("INFO: Broadcast message, ignoring");
                    }
                }

                Reply::Ping
            }
        };

        ret
    }

    pub async fn ping_raw(&self, dst: NodeInfo) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(Request::Ping, self.node_info.clone(), dst)
            .await
    }

    pub async fn store_raw(
        &self,
        dst: NodeInfo,
        k: Key,
        v: &[u8],
    ) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(Request::Store(k, v.to_vec()), self.node_info.clone(), dst)
            .await
    }

    pub async fn find_node_raw(&self, dst: NodeInfo, id: Key) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(Request::FindNode(id), self.node_info.clone(), dst)
            .await
    }

    pub async fn find_value_raw(&self, dst: NodeInfo, k: Key) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(Request::FindValue(k), self.node_info.clone(), dst)
            .await
    }

    pub async fn unicast_raw(&self, dst: NodeInfo, msg: &[u8]) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(Request::Unicast(msg.to_vec()), self.node_info.clone(), dst)
            .await
    }

    pub async fn broadcast_raw(
        &self,
        dst: NodeInfo,
        msg: &[u8],
    ) -> UnboundedReceiver<Option<Reply>> {
        self.rpc
            .lock()
            .await
            .send_req(
                Request::Broadcast(msg.to_vec()),
                self.node_info.clone(),
                dst,
            )
            .await
    }

    pub async fn ping(&self, dst: NodeInfo) -> Option<()> {
        let rep = self.ping_raw(dst.clone()).await.recv().await.unwrap();
        let mut routes = self.routes.lock().await;
        if let Some(Reply::Ping) = rep {
            routes.update(dst);
            Some(())
        } else {
            routes.remove(&dst);
            None
        }
    }

    pub async fn store(&self, dst: NodeInfo, k: Key, v: &[u8]) -> Option<()> {
        let rep = self
            .store_raw(dst.clone(), k, v)
            .await
            .recv()
            .await
            .unwrap();
        let mut routes = self.routes.lock().await;
        if let Some(Reply::Ping) = rep {
            routes.update(dst);
            Some(())
        } else {
            routes.remove(&dst);
            None
        }
    }

    pub async fn find_node(&self, dst: NodeInfo, id: Key) -> Option<Vec<(NodeInfo, Key)>> {
        let rep = self
            .find_node_raw(dst.clone(), id)
            .await
            .recv()
            .await
            .unwrap();
        let mut routes = self.routes.lock().await;
        if let Some(Reply::FindNode(entries)) = rep {
            routes.update(dst);
            Some(entries)
        } else {
            routes.remove(&dst);
            None
        }
    }

    pub async fn find_value(&self, dst: NodeInfo, k: Key) -> Option<FindValueResult> {
        let rep = self
            .find_value_raw(dst.clone(), k)
            .await
            .recv()
            .await
            .unwrap();
        let mut routes = self.routes.lock().await;
        if let Some(Reply::FindValue(res)) = rep {
            routes.update(dst);
            Some(res)
        } else {
            routes.remove(&dst);
            None
        }
    }

    pub async fn unicast(&self, dst: NodeInfo, msg: &[u8]) -> Option<()> {
        let rep = self
            .unicast_raw(dst.clone(), msg)
            .await
            .recv()
            .await
            .unwrap();
        let mut routes = self.routes.lock().await;
        if let Some(Reply::Ping) = rep {
            routes.update(dst);
            Some(())
        } else {
            routes.remove(&dst);
            None
        }
    }

    pub async fn broadcast(&self, msg: &[u8]) -> Vec<NodeInfo> {
        let mut broadcast_tokens = self.broadcast_tokens.lock().await;
        broadcast_tokens.insert(Key::hash(msg, KEY_LEN));
        drop(broadcast_tokens);

        let mut ret = Vec::new();
        let mut reps = Vec::new();

        let mut routes = self.routes.lock().await;
        for bucket in routes.get_buckets() {
            for dst in bucket.iter() {
                if *dst == self.node_info {
                    continue;
                }
                reps.push((
                    self.broadcast_raw(dst.clone(), msg)
                        .await
                        .recv()
                        .await
                        .unwrap(),
                    dst.clone(),
                ));
            }
        }

        for (rep, dst) in reps.drain(..) {
            if let Some(Reply::Ping) = rep {
                ret.push(dst.clone());
                routes.update(dst);
            } else {
                routes.remove(&dst);
            }
        }
        drop(routes);

        ret
    }

    pub async fn lookup_nodes(&self, id: Key) -> Vec<(NodeInfo, Key)> {
        let mut queried = HashSet::new();
        let mut ret = HashSet::new();

        let routes = self.routes.lock().await;
        let mut to_query = BinaryHeap::from(routes.closest_nodes(id.clone(), K_PARAM));
        drop(routes);
        for entry in &to_query {
            queried.insert(entry.clone());
        }

        while !to_query.is_empty() {
            let mut joins = Vec::new();
            let mut queries = Vec::new();
            let mut results = Vec::new();
            for _ in 0..A_PARAM {
                match to_query.pop() {
                    Some(entry) => queries.push(entry),
                    None => break,
                }
            }
            for &(ref ni, _) in &queries {
                let ni = ni.clone();
                joins.push(self.find_node(ni.clone(), id.clone()));
            }
            for j in joins {
                results.push(j.await);
            }
            for (res, query) in results.into_iter().zip(queries) {
                if let Some(entries) = res {
                    ret.insert(query);
                    for entry in entries {
                        if queried.insert(entry.clone()) {
                            to_query.push(entry);
                        }
                    }
                }
            }
        }

        let mut ret = ret.into_iter().collect::<Vec<_>>();
        ret.sort_by(|a, b| a.1.cmp(&b.1));
        ret.truncate(K_PARAM);
        ret
    }

    pub async fn lookup_value(&self, k: Key) -> (Option<Vec<u8>>, Vec<(NodeInfo, Key)>) {
        let id = k.to_hash();
        let mut queried = HashSet::new();
        let mut ret = HashSet::new();

        let routes = self.routes.lock().await;
        let mut to_query = BinaryHeap::from(routes.closest_nodes(id, K_PARAM));
        drop(routes);
        for entry in &to_query {
            queried.insert(entry.clone());
        }

        while !to_query.is_empty() {
            let mut joins = Vec::new();
            let mut queries = Vec::new();
            let mut results = Vec::new();
            for _ in 0..A_PARAM {
                match to_query.pop() {
                    Some(entry) => {
                        queries.push(entry);
                    }
                    None => {
                        break;
                    }
                }
            }
            for &(ref ni, _) in &queries {
                let k = k.clone();
                let ni = ni.clone();
                joins.push(self.find_value(ni.clone(), k));
            }
            for j in joins {
                results.push(j.await);
            }
            for (res, query) in results.into_iter().zip(queries) {
                if let Some(fvres) = res {
                    match fvres {
                        FindValueResult::Nodes(entries) => {
                            ret.insert(query);
                            for entry in entries {
                                if queried.insert(entry.clone()) {
                                    to_query.push(entry);
                                }
                            }
                        }
                        FindValueResult::Value(val) => {
                            let mut ret = ret.into_iter().collect::<Vec<_>>();
                            ret.sort_by(|a, b| a.1.cmp(&b.1));
                            ret.truncate(K_PARAM);
                            return (Some(val), ret);
                        }
                    }
                }
            }
        }

        let mut ret = ret.into_iter().collect::<Vec<_>>();
        ret.sort_by(|a, b| a.1.cmp(&b.1));
        ret.truncate(K_PARAM);

        (None, ret)
    }

    pub async fn put(&self, k: Key, v: &[u8]) {
        let candidates = self.lookup_nodes(k.to_hash()).await;
        let mut res = Vec::new();
        for (node_info, _) in candidates.iter() {
            res.push(self.store(node_info.clone(), k.clone(), v));
        }
        for r in res {
            r.await;
        }
    }

    pub async fn get(&self, k: Key) -> Option<Vec<u8>> {
        let (v_opt, mut nodes) = self.lookup_value(k.clone()).await;
        if let Some(v) = v_opt {
            if let Some((store_target, _)) = nodes.pop() {
                self.store(store_target, k, &v).await;
            } else {
                self.store(self.node_info.clone(), k, &v).await;
            }

            Some(v)
        } else {
            None
        }
    }

    pub async fn show_routes(&self) {
        println!("buckets:");
        for bucket in self.routes.lock().await.get_buckets().iter() {
            print!("[");
            for node in bucket.iter() {
                print!("{:?}, ", node);
            }
            print!("]\n");
        }
    }

    pub async fn show_store(&self) {
        println!("store:");
        for (key, val) in self.store.lock().await.iter() {
            println!(
                "{:?}: {}",
                key,
                String::from_utf8(val.to_vec()).unwrap_or(String::from("<NOT A STRING>"))
            );
        }
    }

    pub async fn show_broadcast_messages(&self) {
        println!("broadcast tokens:");
        for key in self.broadcast_tokens.lock().await.iter() {
            println!("{:?}", key);
        }
    }
}
