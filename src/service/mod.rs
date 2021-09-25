mod network;
mod user_handle;

pub use user_handle::UserHandle;
pub use network::{UserDHT,Publisher,Subscriber};

pub const USER_DHT_KEY_LENGTH: usize= 32;
pub const PUBSUB_DHT_KEY_LENGTH: usize= 64;

pub const TESTNET_USER_DHT: &str = "test_user_dht";
pub const TESTNET_PUBSUB_DHT: &str = "test_pubsub_dht";
pub const MAINNET_USER_DHT: &str = "user_dht";
pub const MAINNET_PUBSUB_DHT: &str = "pubsub_dht";