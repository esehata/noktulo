use std::collections::HashMap;

use crate::user::user::UserAttribute;
use crate::user::{post::Post, user::Address};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserHandle {
    pub user_attr: UserAttribute,
    pub signing_key: [u8; 32],
    pub followings: HashMap<Address, UserAttribute>,
    pub posts: Vec<Post>,
}

impl UserHandle {
    pub fn new(
        user_info: UserAttribute,
        secret_key: [u8; 32],
        followings: HashMap<Address,UserAttribute>,
        posts: &[Post],
    ) -> UserHandle {
        UserHandle {
            user_attr: user_info,
            signing_key: secret_key,
            followings,
            posts: posts.to_vec(),
        }
    }
}
