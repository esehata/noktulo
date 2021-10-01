use std::{net::SocketAddr, sync::Arc};

use tokio::{net::UdpSocket, sync::Mutex};

use crate::{
    crypto::PublicKey,
    kad::{NodeInfo, Rpc},
    service::{Publisher, Subscriber, UserDHT, PUBSUB_DHT_KEY_LENGTH, USER_DHT_KEY_LENGTH},
    user::user::Address,
};

pub struct NetworkController {
    rpc: Arc<Mutex<Rpc>>,

    user_dht: UserDHT,
    pubsub_dht_bootstrap: Vec<NodeInfo>,
}

impl NetworkController {
    pub async fn init(config: Config) -> NetworkController {
        let mut bootstrap_nodeinfo = Vec::new();
        for addr in config.bootstrap {
            let ret = Rpc::get_nodeinfos(addr).await;
            if let Ok(mut v) = ret {
                bootstrap_nodeinfo.append(&mut v);
            }
        }

        let user_dht_bootstrap: Vec<_> = bootstrap_nodeinfo
            .iter()
            .filter(|ni| ni.id.len() == USER_DHT_KEY_LENGTH)
            .cloned()
            .collect();
        let pubsub_dht_bootstrap: Vec<_> = bootstrap_nodeinfo
            .iter()
            .filter(|ni| ni.id.len() == PUBSUB_DHT_KEY_LENGTH)
            .cloned()
            .collect();

        let socket = UdpSocket::bind(config.bind_addr).await.unwrap();
        let rpc = Rpc::new(socket);
        if let Some(addr) = config.nodeinfo_addr {
            rpc.start_nodeinfo_server(addr).await.unwrap();
        }

        let user_dht = UserDHT::start(Arc::new(Mutex::new(rpc.clone())), &user_dht_bootstrap).await;

        NetworkController {
            rpc: Arc::new(Mutex::new(rpc)),
            user_dht,
            pubsub_dht_bootstrap,
        }
    }

    pub async fn create_publisher(&self, pubkey: &PublicKey) -> Publisher {
        self.user_dht.register_pubkey(pubkey).await;
        Publisher::new(
            Address::from(pubkey.clone()),
            self.rpc.clone(),
            &self.pubsub_dht_bootstrap,
        )
        .await
    }

    pub async fn create_subscriber(&self) -> Subscriber {
        Subscriber::new(self.rpc.clone(), &self.pubsub_dht_bootstrap).await
    }

    pub async fn get_pubkey(&self, addr: Address) -> Option<PublicKey> {
        self.user_dht.get_pubkey(addr).await
    }
}

pub struct Config {
    pub bind_addr: SocketAddr,
    pub nodeinfo_addr: Option<SocketAddr>,
    pub bootstrap: Vec<SocketAddr>,
}
