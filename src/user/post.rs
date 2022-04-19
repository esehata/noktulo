use super::user::{Address, UserAttribute};
use crate::crypto::Ed25519Error;
use crate::crypto::PublicKey;
use chrono::Local;
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::convert::TryInto;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignedPost {
    pub addr: Address,
    pub post: Post,
    #[serde(with = "BigArray")]
    pub signature: [u8; 64]
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

impl fmt::Display for SignedPost {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} @{} [{}]:\n{}",
            self.post.user_attr.name,
            self.addr.to_string(),
            Local
                .timestamp(self.post.created_at as i64, 0)
                .format("%Y/%m/%d %H:%M:%S"),
            self.post
        )
    }
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
    pub user_attr: UserAttribute,
    pub id: u128,
    pub content: PostKind,
    pub created_at: u64,
}

impl fmt::Display for Post {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.content {
            PostKind::Hoot(hoot) => {
                write!(f, "{}", hoot)
            }
            PostKind::ReHoot(sigpost) => {
                write!(f, "\"{}\"", sigpost)
            }
            PostKind::Delete(id) => {
                write!(f, "DELETE HOOT ID: {}", id)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostKind {
    Hoot(Hoot),
    ReHoot(Box<SignedPost>),
    Delete(u128),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hoot {
    pub text: String,
    #[serde(default)]
    #[serde(skip_serializing_if="Option::is_none")]
    pub quoted_posts: Option<Box<SignedPost>>,
    #[serde(default)]
    #[serde(skip_serializing_if="Option::is_none")]
    pub reply_to: Option<Box<SignedPost>>,
    #[serde(default)]
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mention_to: Vec<Address>,
}

impl fmt::Display for Hoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(to) = &self.quoted_posts {
            let _ = writeln!(f, "\"{}\"", to);
        }
        if let Some(to) = &self.reply_to {
            let _ = write!(f, "@{} ", to.addr.to_string());
        }
        for to in self.mention_to.iter() {
            let _ = write!(f, "@{} ", to.to_string());
        }
        let _ = writeln!(f, "");

        writeln!(f, "{}", self.text)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn serde_test() {
        use super::Hoot;
        let hoot = Hoot {text: "aaa".to_string(),quoted_posts:None,reply_to:None,mention_to:Vec::new()};
        let ser=serde_json::to_string(&hoot).unwrap();
        println!("{}",ser);
        let de:Hoot = serde_json::from_str(&ser).unwrap();
        println!("{:?}",de);
    }
}