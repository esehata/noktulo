use crate::user::post::Post;
use crate::user::user::UserAttribute;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserHandle {
    pub user_attr: UserAttribute,
    pub signing_key: [u8; 32],
    pub following: Vec<UserAttribute>,
    pub posts: Vec<Post>,
}

impl UserHandle {
    pub fn new(
        user_info: UserAttribute,
        secret_key: [u8; 32],
        following: &[UserAttribute],
        posts: &[Post],
    ) -> UserHandle {
        UserHandle {
            user_attr: user_info,
            signing_key: secret_key,
            following: following.to_vec(),
            posts: posts.to_vec(),
        }
    }
}
