use std::collections::HashMap;

use tokio::sync::mpsc::{error::SendError, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

use crate::{crypto::PublicKey, user::user::Address};

use super::message::ServerMessage;

#[derive(Clone)]
enum ClientStatus {
    NotEstablished,
    SentChallenge {
        pubkey: PublicKey,
        challenge: [u8; 32],
    },
    Established,
}
pub struct ClientInfo {
    tx: UnboundedSender<Message>,
    registered: HashMap<Address, PublicKey>,
    subscripted: Vec<Address>,
    status: ClientStatus,
}

impl ClientInfo {
    pub fn new(tx: UnboundedSender<Message>) -> ClientInfo {
        ClientInfo {
            tx,
            registered: HashMap::new(),
            subscripted: Vec::new(),
            status: ClientStatus::NotEstablished,
        }
    }

    pub fn send(&self, msg: Message) -> Result<(), SendError<Message>> {
        self.tx.send(msg)
    }

    pub fn subscripted_list(&mut self) -> &mut Vec<Address> {
        &mut self.subscripted
    }

    pub fn send_challenge(
        &mut self,
        pubkey: PublicKey,
        challenge: [u8; 32],
    ) -> Result<(), SendError<Message>> {
        self.status = ClientStatus::SentChallenge { pubkey, challenge };
        self.send(Message::Text(
            serde_json::to_string(&ServerMessage::Challenge(challenge)).unwrap(),
        ))
    }

    pub fn send_invalid(&self) -> Result<(), SendError<Message>> {
        self.send(Message::Text(
            serde_json::to_string(&ServerMessage::Invalid).unwrap(),
        ))
    }

    pub fn verify_challenge_sig(&mut self, sig: [u8; 64]) -> Result<PublicKey, ()> {
        if let ClientStatus::SentChallenge { pubkey, challenge } = self.status.clone() {
            if pubkey.verify(&sig, &challenge[..]).is_ok() {
                self.registered
                    .entry(Address::from(pubkey.clone()))
                    .or_insert(pubkey.clone());

                self.status = ClientStatus::Established;
                Ok(pubkey)
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    pub fn is_established(&self) -> bool {
        !self.registered.is_empty()
    }

    pub fn get_sender(&self) -> UnboundedSender<Message> {
        self.tx.clone()
    }

    pub fn get_pubkey(&self, addr: &Address) -> Option<PublicKey> {
        self.registered.get(addr).map(|pk| pk.clone())
    }
}
