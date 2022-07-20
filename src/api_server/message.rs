use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::user::{
    post::SignedPost,
    user::{Address, SignedUserAttribute},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    EstablishReq { addr: [u8; 32], pubkey: [u8; 32] },
    ChallengeResponce(#[serde(with = "BigArray")] [u8; 64]),
    PublicKey([u8; 32]),
    Post(SignedPost),
    SubscribeReq(Address),
    UnsubscribeReq(Address),
    GetUserInfo(Address),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    Success,
    Denied,
    Invalid,
    Subscribed(SignedPost),
    UserInfo(SignedUserAttribute),
    Challenge([u8; 32]),
    Established,
}
