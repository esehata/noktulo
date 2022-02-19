use chrono::Utc;
use log::warn;
use noktulo::cli::Timeline;
use noktulo::service::{Config, NetworkController, UserHandle};
use noktulo::user::user::{Address, SignedUserAttribute, UserAttribute};
use serde_json;
use noktulo::crypto::{PublicKey,SecretKey};
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::fs::{File, OpenOptions, create_dir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();
    let mut app = CLI::init().await.unwrap();
    return app.cli().await;
}

struct CLI {
    controller: NetworkController,
    user_handles: Vec<UserHandle>,
    pubkey_dict: HashMap<Address, PublicKey>,
}

impl CLI {
    pub async fn init() -> io::Result<CLI> {
        let config = Config {
            bind_addr: SocketAddr::from_str("0.0.0.0:6270").unwrap(),
            nodeinfo_addr: Some(SocketAddr::from_str("0.0.0.0:6271").unwrap()),
            bootstrap: Vec::new(),
        };
        let net = NetworkController::init(config).await;

        let _ = create_dir("localdata").await;

        let mut userfile = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("localdata/users")
            .await
            .unwrap();
        let mut buf = vec![];
        userfile.read_to_end(&mut buf).await?;

        let user_handles: Vec<UserHandle> = match serde_json::from_slice(&buf) {
            Ok(e) => e,
            Err(_) => {
                userfile.set_len(0).await.unwrap(); // truncate
                vec![]
            }
        };

        let mut pubkey_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("localdata/pubkeys")
            .await?;
        let mut buf = vec![];
        pubkey_file.read_to_end(&mut buf).await?;

        let pk_bytes: Vec<[u8; 32]> = match serde_json::from_slice(&buf) {
            Ok(e) => e,
            Err(_) => {
                pubkey_file.set_len(0).await.unwrap();
                vec![]
            }
        };

        let mut pubkey_dict = HashMap::new();
        for bytes in pk_bytes {
            if let Ok(pk) = PublicKey::from_bytes(&bytes) {
                let addr = Address::from(pk.clone());
                pubkey_dict.insert(addr, pk);
            }
        }

        Ok(CLI {
            controller: net,
            user_handles,
            pubkey_dict,
        })
    }

    pub async fn cli(&mut self) -> io::Result<()> {
        loop {
            println!("Select a user:");
            for (i, u) in self.user_handles.iter().enumerate() {
                println!("[{}] {}", i, u.sig_attr.attr.name);
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
            io::stdout().flush().unwrap();
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

        let mut userfile = File::create("localdata/users").await?;
        userfile
            .write_all(
                serde_json::to_string(&self.user_handles)
                    .unwrap()
                    .as_bytes(),
            )
            .await?;

        Ok(())
    }

    pub async fn timeline(&mut self, mut user_handle: UserHandle) -> UserHandle {
        let mut timeline = Timeline::new();

        let pk = PublicKey::from(SecretKey::from(user_handle.signing_key));

        let publisher = self.controller.create_publisher(&pk).await;
        let mut subscriber = self.controller.create_subscriber().await;

        for (addr, _) in user_handle.followings.iter() {
            subscriber.subscribe(addr.clone()).await;
        }

        loop {
            print!("> ");
            io::stdout().flush().unwrap();
            let mut command = String::new();
            io::stdin().read_line(&mut command).unwrap();
            let command_t = command.trim();

            match command_t {
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
                                self.pubkey_dict.insert(sigpost.addr.clone(), pubkey.clone());
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
        let public_key = PublicKey::from(secret_key.clone());
        let addr = Address::from(public_key.clone());

        let mut name = String::new();
        let mut description = String::new();

        print!("Name: ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut name).unwrap();
        name = name.trim().to_string();
        print!("Profile: ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut description).unwrap();
        description = description.trim().to_string();

        let created_at: u64 = Utc::now().timestamp().try_into().unwrap();

        let user_attr = UserAttribute::new(&name, created_at, &description);

        let signature = secret_key.sign(&serde_json::to_vec(&user_attr).unwrap());
        let sig_attr = SignedUserAttribute::new(addr, user_attr, signature);
        sig_attr.verify(&public_key).unwrap();

        let user_handle =
            UserHandle::new(sig_attr, secret_key.into(), HashMap::new(), &Vec::new());
        self.user_handles.push(user_handle.clone());

        let mut userfile = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("localdata/users")
            .await?;
        userfile.set_len(0).await?;
        userfile
            .write_all(
                serde_json::to_string(&self.user_handles)
                    .unwrap()
                    .as_bytes(),
            )
            .await?;

        println!("Created new user: {} @{}",user_handle.sig_attr.attr.name,user_handle.sig_attr.addr.to_string());

        Ok(user_handle)
    }

    /* pub async fn run(&mut self) -> io::Result<()> {
        let input = io::stdin();
        println!("bootstrap:");
        let mut buffer = String::new();
        input.read_line(&mut buffer).unwrap();
        let params = buffer.trim_end().split(' ').collect::<Vec<_>>();
        let bootstrap = if params.len() < 2 {
            Vec::new()
        } else {
            vec![NodeInfo {
                id: Key::try_from(params[1]).unwrap(),
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
                    dummy_info.id = Key::try_from(args[2]).unwrap();
                    println!("{:?}", handle.ping(dummy_info.clone()).await);
                }
                "s" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::try_from(args[2]).unwrap();
                    println!(
                        "{:?}",
                        handle
                            .store(dummy_info.clone(), Key::try_from(args[3]).unwrap(), args[4].as_bytes())
                            .await
                    );
                }
                "fn" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::try_from(args[2]).unwrap();
                    println!(
                        "{:?}",
                        handle
                            .find_node(dummy_info.clone(), Key::try_from(args[3]).unwrap())
                            .await
                    );
                }
                "fv" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::try_from(args[2]).unwrap();
                    println!(
                        "{:?}",
                        handle
                            .find_value(dummy_info.clone(), Key::try_from(args[3]).unwrap())
                            .await
                    );
                }
                "ln" => {
                    println!("{:?}", handle.lookup_nodes(Key::try_from(args[1]).unwrap()).await);
                }
                "lv" => {
                    println!("{:?}", handle.lookup_value(Key::try_from(args[1]).unwrap()).await);
                }
                "put" => {
                    println!(
                        "{:?}",
                        handle.put(Key::try_from(args[1]).unwrap(), args[2].as_bytes()).await
                    );
                }
                "get" => {
                    println!("{:?}", handle.get(Key::try_from(args[1]).unwrap()).await);
                }
                "uc" => {
                    dummy_info.addr = args[1].parse().unwrap();
                    dummy_info.id = Key::try_from(args[2]).unwrap();
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
    } */
}
