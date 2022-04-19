use serde::{Serialize, Deserialize};

use crate::user::{post::SignedPost, user::{Address, SignedUserAttribute}};

#[derive(Debug,Serialize,Deserialize)]
pub enum ClientMessage {
    Post(SignedPost),
    SubscribeReq(Address),
    UnsubscribeReq(Address),
    GetUserInfo(Address),
}

#[derive(Debug,Serialize,Deserialize)]
pub enum ServerMessage {
    Success,
    Subscribe(SignedPost),
    UserInfo(SignedUserAttribute),
}