use std::collections::HashMap;

use crate::crypto::{PublicKey, SecretKey};
use crate::user::post::{Hoot, Post, PostKind};
use crate::user::user::{SignedUserAttribute, UserAttribute};
use crate::user::{post::SignedPost, user::Address};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserHandle {
    pub sig_attr: SignedUserAttribute,
    pub signing_key: [u8; 32],
    pub followings: HashMap<Address, Option<UserAttribute>>,
    pub posts: Vec<SignedPost>,
}

impl UserHandle {
    pub fn new(
        sig_attr: SignedUserAttribute,
        signing_key: [u8; 32],
        followings: HashMap<Address, Option<UserAttribute>>,
        posts: &[SignedPost],
    ) -> UserHandle {
        UserHandle {
            sig_attr,
            signing_key,
            followings,
            posts: posts.to_vec(),
        }
    }

    pub fn pubkey(&self) -> PublicKey {
        SecretKey::from(self.signing_key).into()
    }

    pub fn addr(&self) -> Address {
        self.pubkey().into()
    }

    pub fn create_post(&mut self, post: PostKind) -> SignedPost {
        let user_attr = self.sig_attr.attr.clone();

        let mut id = 0;
        if !self.posts.is_empty() {
            id = self.posts.last().unwrap().post.id + 1;
        }

        let created_at = Utc::now().timestamp() as u64;

        let post = Post {
            user_attr,
            id,
            content: post,
            created_at,
        };

        let signature = SecretKey::from(self.signing_key).sign(&serde_json::to_vec(&post).unwrap());

        let sigpost = SignedPost {
            addr: self.addr(),
            post,
            signature: signature.to_vec(),
        };

        self.posts.push(sigpost.clone());

        sigpost
    }

    pub fn hoot(
        &mut self,
        text: String,
        quoted_posts: Option<SignedPost>,
        reply_to: Option<SignedPost>,
        mention_to: Vec<Address>,
    ) -> SignedPost {
        let hoot = Hoot {
            text,
            quoted_posts: quoted_posts.map(|sigpost| Box::new(sigpost)),
            reply_to: reply_to.map(|sigpost| Box::new(sigpost)),
            mention_to,
        };

        self.create_post(PostKind::Hoot(hoot))
    }

    pub fn rehoot(&mut self, post: SignedPost) -> SignedPost {
        self.create_post(PostKind::ReHoot(Box::new(post)))
    }

    pub fn del(&mut self, id: u128) -> Option<SignedPost> {
        let i = self
            .posts
            .iter()
            .position(|sigpost| sigpost.post.id == id)?;
        self.posts.remove(i);
        Some(self.create_post(PostKind::Delete(id)))
    }
}
