mod node;
mod rpc;
mod routing;
mod key;

pub use node::Node;
pub use key::Key;
pub use routing::NodeInfo;
pub use rpc::Rpc;

pub const KEY_LEN: usize = 20;
pub const N_BUCKETS: usize = KEY_LEN * 8;
pub const K_PARAM: usize = 8;
pub const A_PARAM: usize = 3;
pub const MESSAGE_LEN: usize = 8196;
pub const TIME_OUT: u64 = 5000;