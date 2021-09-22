use crate::{account::user::UserInfo};
use crate::account::post::Post;
use serde::{Serialize,Deserialize};

#[derive(Debug,Clone, PartialEq, Eq,Hash,Serialize,Deserialize)]
pub struct  UserHandle {
    pub user_info: UserInfo,
    pub secret_key: [u8;32],
    pub following: Vec<UserInfo>,
    pub posts: Vec<Post>,
}

impl UserHandle {
    pub fn new(user_info: UserInfo, secret_key: [u8;32], following: &[UserInfo], posts: &[Post]) -> UserHandle {
        UserHandle {
            user_info,
            secret_key,
            following: following.to_vec(),
            posts: posts.to_vec(),
        }
    }
}