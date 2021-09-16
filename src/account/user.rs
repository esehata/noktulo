use crate::crypto::*;
use crate::util::base64;
use std::convert::TryInto;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct User {
    pub name: String,
    pub pubkey: PublicKey,
    pub address: Address,
    pub created_at: u64,
    pub description: String,
}

impl User {
    pub fn new(name: &str, pubkey: &PublicKey, created_at: u64, description: &str) -> User {
        User {
            pubkey: pubkey.clone(),
            name: name.to_string(),
            address: Address::new(pubkey),
            created_at,
            description: description.to_string(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Address {
    version: u8,
    address: [u8; 32],
}

impl Address {
    pub fn new(pubkey: &PublicKey) -> Address {
        Address {
            version: 0,
            address: Address::hash(&pubkey.to_bytes_le()),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.address.to_vec()
    }

    pub fn to_string(&self) -> String {
        let payload = [
            &self.version.to_le_bytes()[..],
            &self.address,
            &self.check_sum()[..],
        ]
        .concat();
        String::from_utf8(base64::encode(&payload)).unwrap()
    }

    fn check_sum(&self) -> [u8; 4] {
        let payload = [&self.version.to_le_bytes()[..], &self.address].concat();
        Address::sha3(&Address::sha3(&payload))[..4]
            .try_into()
            .unwrap()
    }

    fn hash(data: &[u8]) -> [u8; 32] {
        // sha3 -> blake2s -> sha3 -> blake2s
        Address::blake2s(&Address::blake2s(&Address::sha3(&Address::sha3(data))))
    }

    fn sha3(data: &[u8]) -> [u8; 64] {
        use sha3::{Digest, Sha3_512};
        Sha3_512::digest(data).as_slice().try_into().unwrap()
    }

    fn blake2s(data: &[u8]) -> [u8; 32] {
        use blake2::{Blake2s, Digest as blake2digest};
        Blake2s::digest(data).as_slice().try_into().unwrap()
    }
}
