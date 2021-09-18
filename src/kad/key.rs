use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_512};
use std::fmt::{Debug, Error, Formatter};
use std::ops::BitXor;

#[derive(Hash, Ord, PartialOrd, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Key(Vec<u8>);

impl Key {
    pub fn from_bytes(bytes: &[u8]) -> Key {
            Key(bytes.to_vec())
    }

    pub fn random(len: usize) -> Key {
        let mut data = vec![0; len];
        let mut rng = ChaCha20Rng::from_entropy();
        rng.fill_bytes(&mut data);
        Key(data)
    }

    pub fn hash(data: &[u8], len: usize) -> Key {
        assert!(len<=64);

        let result = Sha3_512::new().chain(data).finalize();
        let hash = result.iter().take(len).copied().collect();
        Key(hash)
    }

    pub fn to_hash(&self) -> Key {
        let mut hasher = Sha3_512::new();
        hasher.update(self.0.clone());
        let result = hasher.finalize();
        let hash = result[..].to_vec().iter().take(self.0.len()).copied().collect();
        Key(hash)
    }

    pub fn resize(&mut self, new_len: usize) {
        self.0.resize(new_len, 0);
    }

    pub fn zeroes_in_prefix(&self) -> usize {
        for i in 0..self.0.len() {
            if self.0[i] == 0 {
                continue;
            }
            for j in (0..8).rev() {
                if (self.0[i] >> j) & 0x1 != 0 {
                    return i * 8 + j;
                }
            }
        }
        self.0.len() * 8 - 1
    }

    pub fn is_prefix(&self, other: &Key) -> bool {
        if self.len() > other.len() {
            false
        } else {
            for (i,b) in self.0.iter().enumerate() {
                if *b != other.0[i] {
                    return false;
                }
            }

            true
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl BitXor for Key {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        assert!(self.0.len()==rhs.0.len());

        let mut v = Vec::new();
        for i in 0..self.0.len() {
            v.push(self.0[i] ^ rhs.0[i]);
        }

        Key(v)
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Key {
        let mut ret = Vec::new();

        for (i, e) in s.chars().enumerate() {
            let n = e.to_digit(16).unwrap();

            if i & 1 == 0 {
                ret.push((n as u8) << 4);
            } else {
                *ret.last_mut().unwrap() += n as u8;
            }
        }

        //ret.reverse();

        Key(ret)
    }
}

impl Debug for Key {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        for x in self.0.iter() {
            write!(f, "{0:02x}", x).unwrap();
        }
        Ok(())
    }
}