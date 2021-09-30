use std::convert::TryInto;

use crate::crypto::{Ed25519Error, PublicKey};

use super::{
    post::Post,
    user::{Address, UserAttribute},
};
use serde::{Deserialize, Serialize};
use serde_json;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserMessage {
    pub addr: Address,
    pub msg: MessageKind,
    pub signature: [u8; 64],
}

impl UserMessage {
    pub fn encode(&self) -> Vec<u8> {
        let a: [u8; 32] = self.addr.clone().into();
        let m = serde_json::to_vec(&self.msg).unwrap();
        [&a[..], &m.len().to_le_bytes(), &m, &self.signature].concat()
    }

    pub fn decode(v: &[u8]) -> Result<UserMessage, DecodeError> {
        if v.len() < 100 {
            Err(DecodeError::Size)
        } else {
            let len_m = usize::from_le_bytes(v[32..36].try_into().unwrap());
            if v.len() < 100 + len_m as usize {
                Err(DecodeError::Size)
            } else {
                let addr_bytes:[u8;32] = v[..32].try_into().unwrap();
                let addr = Address::from(addr_bytes);
                if let Ok(msg) = serde_json::from_slice(&v[36..36 + len_m].to_vec()) {
                    let signature = v[36 + len_m..36 + len_m + 64].try_into().unwrap();
                    
                    Ok(UserMessage {
                        addr,
                        msg,
                        signature,
                    })
                } else {
                    Err(DecodeError::Message)
                }
            }
        }
    }

    pub fn verify(&self, pubkey: &PublicKey) -> Result<(),VerifyError> {
        let addr = Address::from(pubkey.clone());
        
        if self.addr != addr {
            Err(VerifyError::Address)
        } else {
            pubkey.verify(&self.signature, &serde_json::to_vec(&self.msg).unwrap()).map_err(|e|VerifyError::Signature(e))
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

#[derive(Debug,Error)]
pub enum VerifyError {
    #[error("Invalid address")]
    Address,
    #[error("Invalid signature")]
    Signature(Ed25519Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageKind {
    Post(Post),
    Attr(UserAttribute),
}
