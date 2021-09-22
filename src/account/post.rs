use super::user::{Address,UserInfo};
use serde::{Serialize,Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserMessage {
    pub user: UserInfo,
    pub id: u128,
    pub post_bytes: Vec<u8>, // serialized data of Post
    pub created_at: u64,
    pub signature: Vec<u8>, // 64 bytes
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Post {
    Hoot(Hoot),
    ReHoot(ReHoot),
    Delete(u128),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hoot {
    pub text: String,
    pub quoted_posts: Option<UserMessage>,
    pub reply_to: Option<ReplyTo>,
    pub mention_to: Option<Vec<Address>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReHoot {
    pub post: UserMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReplyTo {
    pub reply_to_user: UserInfo,
    pub reply_to_id: [u8; 32],
}
