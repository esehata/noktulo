use super::user::{Address, UserAttribute};
use crate::crypto::Ed25519Error;
use crate::crypto::PublicKey;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignedPost {
    pub addr: Address,
    pub post: Post,
    pub signature: Vec<u8>, // 64 bytes
}

impl SignedPost {
    pub fn verify(&self, pubkey: &PublicKey) -> Result<(), VerifyError> {
        let addr = Address::from(pubkey.clone());

        if self.addr != addr {
            Err(VerifyError::Address)
        } else {
            if self.signature.len() != 64 {
                Err(VerifyError::Size)
            } else {
                pubkey
                    .verify(
                        &self.signature[..].try_into().unwrap(),
                        &serde_json::to_vec(&self.post).unwrap(),
                    )
                    .map_err(|e| VerifyError::Signature(e))
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("Invalid size")]
    Size,
    #[error("Invalid message")]
    Message,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("Invalid address")]
    Address,
    #[error("Invalid signature")]
    Signature(Ed25519Error),
    #[error("Invalid size")]
    Size,
}

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
    ReHoot(SignedPost),
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
pub struct ReplyTo {
    pub reply_to_user: UserAttribute,
    pub reply_to_id: [u8; 32],
}
