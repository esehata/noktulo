use crate::kad::Key;
use crate::util::base64;
use crate::{crypto::*, util::base64::Base64Error};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignedUserAttribute {
    pub addr: Address,
    pub attr: UserAttribute,
    pub signature: Vec<u8>, // 64 bytes
}

impl SignedUserAttribute {
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
                        &serde_json::to_vec(&self.attr).unwrap(),
                    )
                    .map_err(|e| VerifyError::Signature(e))
            }
        }
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
pub struct UserAttribute {
    pub name: String,
    pub created_at: u64,
    pub description: String,
}

impl UserAttribute {
    pub fn new(
        public_key: [u8; 32],
        name: &str,
        created_at: u64,
        description: &str,
        signature: [u8; 64],
    ) -> Result<UserAttribute, Ed25519Error> {
        if PublicKey::from_bytes(&public_key)?
            .verify(
                &signature,
                &[
                    name.as_bytes(),
                    &created_at.to_le_bytes(),
                    description.as_bytes(),
                ]
                .concat(),
            )
            .is_ok()
        {
            Err(Ed25519Error::Signature)
        } else {
            Ok(UserAttribute {
                name: name.to_string(),
                created_at,
                description: description.to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    prefix: u8,
    address: [u8; 32],
}

impl Address {
    pub fn new(address: [u8; 32]) -> Address {
        Address { prefix: 0, address }
    }

    pub fn from_str(s: &str) -> Result<Address, AddressError> {
        match base64::decode(s.as_bytes()) {
            Ok(b) => {
                if b.len() != 37 {
                    Err(AddressError::Length)
                } else {
                    let version = b[0];
                    let addr = &b[1..33];
                    let checksum = &b[33..];
                    let ret = Address {
                        prefix: version,
                        address: addr.try_into().unwrap(),
                    };
                    if checksum != ret.check_sum() {
                        Err(AddressError::Checksum)
                    } else {
                        Ok(ret)
                    }
                }
            }
            Err(e) => Err(AddressError::Base64(e)),
        }
    }

    pub fn to_string(&self) -> String {
        let payload = [
            &self.prefix.to_le_bytes()[..],
            &self.address,
            &self.check_sum()[..],
        ]
        .concat();
        String::from_utf8(base64::encode(&payload)).unwrap()
    }

    fn check_sum(&self) -> [u8; 4] {
        let payload = [&self.prefix.to_le_bytes()[..], &self.address].concat();
        Address::sha3(&Address::sha3(&payload))[..4]
            .try_into()
            .unwrap()
    }

    fn hash(data: [u8; 32]) -> [u8; 32] {
        // sha3 -> blake2s -> sha3 -> blake2s
        Address::blake2s(&Address::blake2s(&Address::sha3(&Address::sha3(&data))))
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

impl From<PublicKey> for Address {
    fn from(pubkey: PublicKey) -> Address {
        Address {
            prefix: 0,
            address: Address::hash(pubkey.into()),
        }
    }
}

impl From<[u8; 32]> for Address {
    fn from(b: [u8; 32]) -> Address {
        Address {
            prefix: 0,
            address: b[..].try_into().unwrap(),
        }
    }
}

impl From<Address> for [u8; 32] {
    fn from(addr: Address) -> [u8; 32] {
        addr.address
    }
}

impl From<Address> for Key {
    fn from(addr: Address) -> Key {
        let b: [u8; 32] = addr.into();
        Key::from(&b[..])
    }
}

#[derive(Debug, Error)]
pub enum AddressError {
    #[error("Invalid length")]
    Length,
    #[error("Invalid checksum")]
    Checksum,
    #[error("Invalid character")]
    Base64(Base64Error),
}
