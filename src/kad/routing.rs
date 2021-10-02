use super::key::Key;
use super::K_PARAM;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::vec::Vec;

#[derive(Hash, Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: Key,
    pub addr: SocketAddr,
    pub net_id: String,
}

#[derive(Debug)]
pub struct RoutingTable {
    key_len: usize,
    node_info: NodeInfo,
    buckets: Vec<Vec<NodeInfo>>,
}

impl RoutingTable {
    pub fn new(node_info: &NodeInfo, key_len: usize) -> RoutingTable {
        assert_eq!(node_info.id.len(), key_len);
        let mut buckets = Vec::new();
        for _ in 0..key_len * 8 {
            buckets.push(Vec::new());
        }
        let mut ret = RoutingTable {
            key_len,
            node_info: node_info.clone(),
            buckets,
        };
        ret.update(node_info.clone());
        ret
    }

    pub fn update(&mut self, node_info: NodeInfo) -> Option<NodeInfo> {
        assert_eq!(self.key_len, node_info.id.len());
        let bucket_index = self.lookup_bucket_index(node_info.id.clone());
        let bucket = &mut self.buckets[bucket_index];
        let node_index = bucket.iter().position(|x| x.id == node_info.id);
        match node_index {
            Some(i) => {
                let tmp = bucket.remove(i);
                bucket.push(tmp);
            }
            None => {
                if bucket.len() < K_PARAM {
                    bucket.push(node_info);
                } else {
                    // if bucket is full, return the first element, and caller pings the node and re-update routes
                    return Some(bucket.first().unwrap().clone());
                }
            }
        }

        None
    }

    pub fn closest_nodes(&self, item: Key, count: usize) -> Vec<(NodeInfo, Key)> {
        assert_eq!(self.key_len, item.len());
        if count == 0 {
            return Vec::new();
        }
        let mut ret = Vec::with_capacity(count);
        for bucket in &self.buckets {
            for node_info in bucket {
                ret.push((node_info.clone(), node_info.id.clone() ^ item.clone()));
            }
        }
        ret.sort_by(|a, b| a.1.cmp(&b.1));
        ret.truncate(count);
        ret
    }

    pub fn get_buckets(&self) -> &Vec<Vec<NodeInfo>> {
        &self.buckets
    }

    pub fn remove(&mut self, node_info: &NodeInfo) {
        assert_eq!(self.key_len, node_info.id.len());
        let bucket_index = self.lookup_bucket_index(node_info.id.clone());
        if let Some(item_index) = self.buckets[bucket_index]
            .iter()
            .position(|x| x == node_info)
        {
            self.buckets[bucket_index].remove(item_index);
        } else {
            println!("WARN: Tried to remove routing entry that does not exist.");
        }
    }

    fn lookup_bucket_index(&self, item: Key) -> usize {
        (self.node_info.id.clone() ^ item.clone()).zeroes_in_prefix()
    }
}
