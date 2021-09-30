use super::user::{Address,UserAttribute};
use serde::{Serialize,Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Post {
    pub user: UserAttribute,
    pub id: u128,
    pub post_bytes: Vec<u8>, // serialized data of Post
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostKind {
    Hoot(Hoot),
    ReHoot(ReHoot),
    Delete(u128),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hoot {
    pub text: String,
    pub quoted_posts: Option<Post>,
    pub reply_to: Option<ReplyTo>,
    pub mention_to: Option<Vec<Address>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReHoot {
    pub post: Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReplyTo {
    pub reply_to_user: UserAttribute,
    pub reply_to_id: [u8; 32],
}
