use crate::crypto::*;
use crate::util::base64;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    pub public_key: [u8; 32],
    pub address: Address,
    pub created_at: u64,
    pub description: String,
    pub signature: Vec<u8>, // 64 bytes, Sign(name|created_at|description)
}

impl UserInfo {
    pub fn new(
        name: &str,
        public_key: [u8; 32],
        created_at: u64,
        description: &str,
        signature: [u8; 64],
    ) -> Result<UserInfo, &'static str> {
        if PublicKey::from_bytes(&public_key)
            .verify(
                &signature,
                &[
                    name.as_bytes(),
                    &created_at.to_le_bytes(),
                    description.as_bytes(),
                ]
                .concat(),
            )
            .unwrap()
        {
            Err("Invalid signature!")
        } else {
            Ok(UserInfo {
                public_key: public_key,
                name: name.to_string(),
                address: Address::from_public_key(&PublicKey::from_bytes(&public_key)),
                created_at,
                description: description.to_string(),
                signature: signature.to_vec(),
            })
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    version: u8,
    address: [u8; 32],
}

impl Address {
    pub fn new(address: [u8;32]) -> Address {
        Address{
            version:0,
            address,
        }
    }

    pub fn from_public_key(pubkey: &PublicKey) -> Address {
        Address {
            version: 0,
            address: Address::hash(&pubkey.to_bytes()),
        }
    }

    pub fn from_string(s: &str) -> Result<Address, &'static str> {
        match base64::decode(s.as_bytes()) {
            Ok(b) => {
                if b.len() != 37 {
                    Err("Invalid length")
                } else {
                    let version = b[0];
                    let addr = &b[1..33];
                    let checksum = &b[33..];
                    let ret = Address {
                        version: version,
                        address: addr.try_into().unwrap(),
                    };
                    if checksum != ret.check_sum() {
                        Err("Invalid checksum")
                    } else {
                        Ok(ret)
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    pub fn from_bytes(b: [u8;33]) -> Address {
        let version = b[0];
        let address = &b[1..];
        Address { version, address:address.try_into().unwrap() }
    }

    pub fn to_bytes(&self) -> [u8;33] {
        let a = [self.version];
        [&a[..], &self.address.to_vec()].concat().try_into().unwrap()
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
