use super::user::{Address,User};



#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hoot {
    pub user: User,
    pub created_at: u64,
    pub text: String,
    pub id: [u8; 32],
    pub quoted_posts: Option<Box<Hoot>>,
    pub reply_to: Option<ReplyTo>,
    pub mention_to: Option<Vec<Address>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReHoot {
    pub post: Hoot,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplyTo {
    pub reply_to_user: Address,
    pub reply_to_id: [u8; 32],
}
