use chrono::Utc;
use noktulo::crypto::{PublicKey, SecretKey};
use noktulo::kad::*;
use noktulo::service::{Publisher, Subscriber, UserDHT, UserHandle, TESTNET_USER_DHT};
use noktulo::user::user::{Address, UserAttribute};
use serde_json;
use std::convert::TryInto;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();
    let mut app = App::new();
    app.run().await
}

struct App {}

impl App {
    pub fn new() -> App {
        App {}
    }

    pub async fn cli(bootstrap: Option<SocketAddr>) -> io::Result<()> {
        let mut userfile = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("users")
            .await?;
        let mut buf = vec![];
        userfile.read_to_end(&mut buf).await?;

        let mut users: Vec<UserHandle> = serde_json::from_slice(&buf).unwrap();
        loop {
            println!("Select a user:");
            for (i, u) in users.iter().enumerate() {
                println!("[{}] {}", i, u.user_attr.name);
            }
            println!(
                r"or
        [{}] Create a new account
        [{}] Quit
        ",
                users.len(),
                users.len() + 1
            );

            print!("Input: ");
            let mut s = String::new();
            io::stdin().read_line(&mut s).unwrap();
            let index: usize = s.trim().parse().unwrap();

            if index < users.len() {
                let user_handle = users[index].clone();
                App::timeline(user_handle, bootstrap).await;
            } else if index == users.len() {
                let user_handle = App::create_new_user();
                users.push(user_handle);
                userfile.set_len(0).await?;
                userfile
                    .write_all(serde_json::to_string(&users).unwrap().as_bytes())
                    .await?;
            } else if index == users.len() + 1 {
                break;
            } else {
                panic!("invalid index!");
            }
        }

        Ok(())
    }

    pub async fn timeline(user_handle: UserHandle, bootstrap: Option<SocketAddr>) {
        let mut bootstrap_nodeinfo = Vec::new();
        if let Some(addr) = bootstrap {
            let ret = Rpc::get_nodeinfos(addr).await;
            if let Ok(v) = ret {
                bootstrap_nodeinfo = v;
            }
        }

        let user_dht_bootstrap: Vec<_> = bootstrap_nodeinfo
            .iter()
            .filter(|ni| ni.id.len() == 32)
            .cloned()
            .collect();

        let pubsub_dht_bootstrap: Vec<_> = bootstrap_nodeinfo
            .iter()
            .filter(|ni| ni.id.len() == 64)
            .cloned()
            .collect();

        let socket = UdpSocket::bind("0.0.0.0:6270").await.unwrap();
        let rpc = Arc::new(Mutex::new(Rpc::new(socket)));

        let user_dht = UserDHT::start(rpc.clone(), &user_dht_bootstrap).await;

        let addr = Address::from(PublicKey::from(SecretKey::from(user_handle.signing_key)));

        let publisher = Publisher::new(addr, rpc.clone(), &pubsub_dht_bootstrap).await;
        let subscriber = Subscriber::new(rpc.clone(), &bootstrap_nodeinfo).await;


    }

    pub fn create_new_user() -> UserHandle {
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

        let signature = secret_key
            .sign(
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

        UserHandle::new(user_attr, secret_key.into(), &Vec::new(), &Vec::new())
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
