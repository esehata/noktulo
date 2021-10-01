use chrono::Utc;
use log::warn;
use noktulo::cli::Timeline;
use noktulo::crypto::{PublicKey, SecretKey};
use noktulo::kad::*;
use noktulo::service::{Config, NetworkController, UserHandle, TESTNET_USER_DHT};
use noktulo::user::user::{Address, UserAttribute};
use serde_json;
use std::collections::HashMap;
use std::convert::TryInto;
use std::io;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();
    let mut app = App::init().await.unwrap();
    app.cli().await
}

struct App {
    controller: NetworkController,
    user_handles: Vec<UserHandle>,
    pubkey_dict: HashMap<Address, PublicKey>,
}

impl App {
    pub async fn init() -> io::Result<App> {
        let config = Config {
            bind_addr: SocketAddr::from_str("0.0.0.0:6270").unwrap(),
            nodeinfo_addr: Some(SocketAddr::from_str("0.0.0.0:6271").unwrap()),
            bootstrap: Vec::new(),
        };
        let nok = NetworkController::init(config).await;

        let mut userfile = OpenOptions::new()
            .read(true)
            .create(true)
            .open("users")
            .await?;
        let mut buf = vec![];
        userfile.read_to_end(&mut buf).await?;

        let user_handles: Vec<UserHandle> = serde_json::from_slice(&buf).unwrap();

        let mut pubkey_file = OpenOptions::new()
            .read(true)
            .create(true)
            .open("pubkeys")
            .await?;
        let mut buf = vec![];
        pubkey_file.read_to_end(&mut buf).await?;

        let pk_bytes: Vec<[u8; 32]> = serde_json::from_slice(&buf).unwrap();
        let mut pubkey_dict = HashMap::new();
        for bytes in pk_bytes {
            if let Ok(pk) = PublicKey::from_bytes(&bytes) {
                let addr = Address::from(pk.clone());
                pubkey_dict.insert(addr, pk);
            }
        }

        Ok(App {
            controller: nok,
            user_handles,
            pubkey_dict,
        })
    }

    pub async fn cli(&mut self) -> io::Result<()> {
        loop {
            println!("Select a user:");
            for (i, u) in self.user_handles.iter().enumerate() {
                println!("[{}] {}", i, u.user_attr.name);
            }
            println!(
                r"or
        [{}] Create a new account
        [{}] Quit
        ",
                self.user_handles.len(),
                self.user_handles.len() + 1
            );

            print!("Input: ");
            let mut s = String::new();
            io::stdin().read_line(&mut s).unwrap();
            let index: usize = s.trim().parse().unwrap();

            if index < self.user_handles.len() {
                let user_handle = self.user_handles[index].clone();
                let new_handle = self.timeline(user_handle).await;
                self.user_handles[index] = new_handle;
            } else if index == self.user_handles.len() {
                self.create_new_user().await?;
            } else if index == self.user_handles.len() + 1 {
                break;
            } else {
                println!("invalid index!");
            }
        }

        let mut userfile = File::create("users").await?;
        userfile
            .write_all(
                serde_json::to_string(&self.user_handles)
                    .unwrap()
                    .as_bytes(),
            )
            .await?;

        Ok(())
    }

    pub async fn timeline(&self, mut user_handle: UserHandle) -> UserHandle {
        let mut timeline = Timeline::new();

        let pk = PublicKey::from(SecretKey::from(user_handle.signing_key));

        let publisher = self.controller.create_publisher(&pk).await;
        let mut subscriber = self.controller.create_subscriber().await;

        for (addr, _) in user_handle.followings.iter() {
            subscriber.subscribe(addr.clone()).await;
        }

        loop {
            print!("> ");
            let mut command = String::new();
            io::stdin().read_line(&mut command).unwrap();

            match command.as_str() {
                "update" => {
                    let sigposts = subscriber.get_new_message().await;
                    for sigpost in sigposts {
                        let pubkey;
                        if let Some(pk) = self.pubkey_dict.get(&sigpost.addr) {
                            pubkey = pk.clone();
                        } else {
                            if let Some(pk) = self.controller.get_pubkey(sigpost.addr.clone()).await
                            {
                                pubkey = pk;
                            } else {
                                warn!("Not found the public key, ignoring.");
                                continue;
                            }
                        }

                        if sigpost.verify(&pubkey).is_ok() {
                            user_handle
                                .followings
                                .insert(sigpost.addr.clone(), Some(sigpost.post.user_attr.clone()));
                            timeline.push(sigpost);
                        }
                    }
                }
                "hoot" => {
                    let mut text = String::new();
                    io::stdin().read_line(&mut text).unwrap();
                    let sigpost = user_handle.hoot(text, None, None, vec![]);
                    publisher
                        .publish(&serde_json::to_vec(&sigpost).unwrap(), &user_handle.addr())
                        .await;
                }
                "rehoot" => {
                    let mut index_s = String::new();
                    io::stdin().read_line(&mut index_s).unwrap();
                    if let Ok(index) = index_s.parse::<usize>() {
                        if let Some(sigpost) = timeline.get(index) {
                            let sigpost = user_handle.rehoot(sigpost.clone());
                            publisher
                                .publish(
                                    &serde_json::to_vec(&sigpost).unwrap(),
                                    &user_handle.addr(),
                                )
                                .await;
                        } else {
                            println!("Not found");
                        }
                    } else {
                        println!("Invalid input");
                    }
                }
                "del" => {
                    let mut id_s = String::new();
                    io::stdin().read_line(&mut id_s).unwrap();
                    if let Ok(id) = id_s.parse::<u128>() {
                        if let Some(sigpost) = user_handle.del(id) {
                            publisher
                                .publish(
                                    &serde_json::to_vec(&sigpost).unwrap(),
                                    &user_handle.addr(),
                                )
                                .await;
                        } else {
                            println!("Not found");
                        }
                    } else {
                        println!("Invalid input");
                    }
                }
                "follow" => {
                    let mut addr_s = String::new();
                    io::stdin().read_line(&mut addr_s).unwrap();
                    if let Ok(addr) = Address::from_str(&addr_s) {
                        if !user_handle.followings.contains_key(&addr) {
                            user_handle.followings.insert(addr.clone(), None);
                        }
                        subscriber.subscribe(addr).await;
                    } else {
                        println!("Invalid address");
                    }
                }
                "unfollow" => {
                    let mut addr_s = String::new();
                    io::stdin().read_line(&mut addr_s).unwrap();
                    if let Ok(addr) = Address::from_str(&addr_s) {
                        if user_handle.followings.contains_key(&addr) {
                            user_handle.followings.remove(&addr);
                        }
                        subscriber.stop_subscription(&addr).await;
                    } else {
                        println!("Invalid address");
                    }
                }
                "quit" => break,
                _ => (),
            }
        }
        user_handle
    }

    pub async fn create_new_user(&mut self) -> io::Result<UserHandle> {
        let secret_key = SecretKey::random();
        let public_key: PublicKey = secret_key.clone().into();
        let created_at: u64 = Utc::now().timestamp().try_into().unwrap();
        let mut name = String::new();
        let mut description = String::new();

        print!("Name: ");
        io::stdin().read_line(&mut name).unwrap();
        name = name.trim().to_string();
        print!("Profile: ");
        io::stdin().read_line(&mut description).unwrap();
        description = description.trim().to_string();

        let signature = secret_key.sign(
            &[
                name.as_bytes(),
                &created_at.to_le_bytes(),
                description.as_bytes(),
            ]
            .concat(),
        );

        let user_attr = UserAttribute::new(
            public_key.into(),
            &name,
            created_at,
            &description,
            signature,
        )
        .unwrap();

        let user_handle =
            UserHandle::new(user_attr, secret_key.into(), HashMap::new(), &Vec::new());
        self.user_handles.push(user_handle.clone());

        let mut userfile = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("users")
            .await?;
        userfile.set_len(0).await?;
        userfile
            .write_all(
                serde_json::to_string(&self.user_handles)
                    .unwrap()
                    .as_bytes(),
            )
            .await?;

        Ok(user_handle)
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let input = io::stdin();
        println!("bootstrap:");
        let mut buffer = String::new();
        input.read_line(&mut buffer).unwrap();
        let params = buffer.trim_end().split(' ').collect::<Vec<_>>();
        let bootstrap = if params.len() < 2 {
            Vec::new()
        } else {
            vec![NodeInfo {
                id: Key::from(params[1]),
                addr: params[0].parse().unwrap(),
                net_id: String::from(TESTNET_USER_DHT),
            }]
        };

        buffer.clear();
        println!("port:");
        input.read_line(&mut buffer).unwrap();
        let port = if buffer.trim() == "" {
            "8080"
        } else {
            &buffer.trim()
        };

        let socket = UdpSocket::bind("127.0.0.1:".to_string() + port)
            .await
            .unwrap();
        let rpc = Arc::new(Mutex::new(Rpc::new(socket)));
        let (tx, _rx) = mpsc::unbounded_channel();

        let handle = Node::start(
            String::from(TESTNET_USER_DHT),
            TOKEN_KEY_LEN,
            Key::random(TOKEN_KEY_LEN),
            Arc::new(|_| true),
            rpc.clone(),
            tx,
            &bootstrap,
        )
        .await;

        let mut dummy_info = NodeInfo {
            net_id: String::from(TESTNET_USER_DHT),
            addr: "127.0.0.1:8080".parse().unwrap(),
            id: Key::random(TOKEN_KEY_LEN),
        };

        loop {
            let mut buffer = String::new();
            if input.read_line(&mut buffer).is_err() {
                break;
            }
            let args = buffer.trim_end().split(' ').collect::<Vec<_>>();
            match args[0].as_ref() {
                "p" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::from(args[2]);
                    println!("{:?}", handle.ping(dummy_info.clone()).await);
                }
                "s" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::from(args[2]);
                    println!(
                        "{:?}",
                        handle
                            .store(dummy_info.clone(), Key::from(args[3]), args[4].as_bytes())
                            .await
                    );
                }
                "fn" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::from(args[2]);
                    println!(
                        "{:?}",
                        handle
                            .find_node(dummy_info.clone(), Key::from(args[3]))
                            .await
                    );
                }
                "fv" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::from(args[2]);
                    println!(
                        "{:?}",
                        handle
                            .find_value(dummy_info.clone(), Key::from(args[3]))
                            .await
                    );
                }
                "ln" => {
                    println!("{:?}", handle.lookup_nodes(Key::from(args[1])).await);
                }
                "lv" => {
                    println!("{:?}", handle.lookup_value(Key::from(args[1])).await);
                }
                "put" => {
                    println!(
                        "{:?}",
                        handle.put(Key::from(args[1]), args[2].as_bytes()).await
                    );
                }
                "get" => {
                    println!("{:?}", handle.get(Key::from(args[1])).await);
                }
                "uc" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::from(args[2]);
                    println!(
                        "{:?}",
                        handle.unicast(dummy_info.clone(), args[3].as_bytes()).await
                    );
                }
                "bc" => {
                    println!("{:?}", handle.broadcast(args[1].as_bytes(),).await);
                }
                "sr" => {
                    handle.show_routes().await;
                }
                "ss" => {
                    handle.show_store().await;
                }
                "sb" => {
                    handle.show_broadcast_messages().await;
                }
                _ => {
                    println!("no match");
                }
            }
        }
        Ok(())
    }
}
