use num_bigint::{BigUint, ToBigUint};
use once_cell::sync::Lazy;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
//use sha3::{Digest, Sha3_512};
use sha2::{Digest, Sha512};
use std::convert::TryInto;
use std::fmt;
use std::ops::Add;
use thiserror::Error;

const B: u64 = 256;

fn h(m: &[u8]) -> [u8; 64] {
    //Sha3_512::digest(m).as_slice().try_into().unwrap()
    Sha512::digest(m).as_slice().try_into().unwrap()
}

// 2^255-19
static Q: Lazy<BigUint> = Lazy::new(|| (1.to_biguint().unwrap() << 255) - 19u8);

// 2^252 + 27742317777372353535851937790883648493
static L: Lazy<BigUint> = Lazy::new(|| {
    (1.to_biguint().unwrap() << 252)
        + BigUint::parse_bytes(b"27742317777372353535851937790883648493", 10).unwrap()
});

fn inv(x: BigUint) -> BigUint {
    x.modpow(&((*Q).clone() - 2u8), &(*Q))
}

// -121665/121666 mod q
static D: Lazy<BigUint> =
    Lazy::new(|| (((*Q).clone() - 121665u64) * inv(121666.to_biguint().unwrap())) % (*Q).clone());

// 2^((q-1)/4) mod q
static I: Lazy<BigUint> = Lazy::new(|| {
    2.to_biguint()
        .unwrap()
        .modpow(&(((*Q).clone() - 1u8) / 4u8), &(*Q))
});

fn h_int(m: &[u8]) -> BigUint {
    let h = h(m);
    BigUint::from_bytes_le(&h)
}

fn xrecover(y: &BigUint) -> BigUint {
    // x^2 = (y^2-1)/(dy^2+1)
    let xx = (y.clone() * y.clone() + ((*Q).clone() - 1u8))
        * inv((*D).clone() * y.clone() * y.clone() + 1u8)
        % (*Q).clone();

    // x = (x^2)^{(q+3)/8} mod q
    let mut x = xx.modpow(&(((*Q).clone() + 3u8) / 8u8), &(*Q));

    // (x*x - x^2) mod q != 0
    if (x.clone() * x.clone() + ((*Q).clone() - xx)) % (*Q).clone() != 0.to_biguint().unwrap() {
        // x = (x*I) mod q
        x = (x * (*I).clone()) % (*Q).clone()
    }

    // x%2 != 0
    if x.bit(0) {
        // x = -x
        x = (*Q).clone() - x;
    }

    x
}

static BASE_POINT: Lazy<Ed25519Point> = Lazy::new(|| {
    let y = 4.to_biguint().unwrap() * inv(5.to_biguint().unwrap()) % (*Q).clone();
    let x = xrecover(&y) % (*Q).clone();
    Ed25519Point::new(x, y).unwrap()
});

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct SecretKey {
    sk: [u8; 32],
}

impl SecretKey {
    pub fn random() -> SecretKey {
        let mut sk = [0; 32];
        ChaCha20Rng::from_entropy().fill_bytes(&mut sk);
        SecretKey { sk }
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> SecretKey {
        SecretKey { sk: bytes.clone() }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.sk.clone()
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        // 512bit
        let h = h(&self.sk);
        // 下位256bitを取り出して整数とする
        let mut s = BigUint::from_bytes_le(&h[..(B as usize) / 8]);

        // 下位3bitを消す
        s.set_bit(0, false);
        s.set_bit(1, false);
        s.set_bit(2, false);

        // b-2 bit目は立ててb-1 bit目は無視する
        s.set_bit(B - 2, true);
        s.set_bit(B - 1, false);

        // 上位256bitを取り出す
        let r = h_int(&[&h[(B as usize) / 8..], message].concat());
        let rr = (*BASE_POINT).clone().scalar_mul(r.clone()).encode();

        let pk = self.public_key().to_bytes();
        let k = h_int(&[&rr[..], &pk[..], message].concat());

        let ss = (r + k * s) % (*L).clone();
        let ss_bytes:[u8;32]=ss.to_bytes_le().try_into().unwrap();

        [&rr[..], &ss_bytes[..]]
            .concat()
            .try_into()
            .unwrap()
    }

    pub fn public_key(&self) -> PublicKey {
        // 512bit
        let h = h(&self.sk);
        // 下位256bitを取り出して整数とする
        let mut a = BigUint::from_bytes_le(&h[..(B as usize) / 8]);

        // 下位3bitを消す
        a.set_bit(0, false);
        a.set_bit(1, false);
        a.set_bit(2, false);

        // b-2 bit目は立ててb-1 bit目は無視する
        a.set_bit(B - 2, true);
        a.set_bit(B - 1, false);

        let pk = (*BASE_POINT).clone().scalar_mul(a);
        assert!(pk.is_on_curve());

        PublicKey { pk: pk.encode() }
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let masked = format!("This data doesn't display for privacy");
        f.debug_struct("SecretKey").field("sk", &masked).finish()
    }
}

impl From<[u8; 32]> for SecretKey {
    fn from(bytes: [u8; 32]) -> SecretKey {
        SecretKey { sk: bytes }
    }
}

impl From<SecretKey> for [u8; 32] {
    fn from(sk: SecretKey) -> [u8; 32] {
        sk.sk
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicKey {
    pk: [u8; 32],
}

impl PublicKey {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<PublicKey, Ed25519Error> {
        match Ed25519Point::decode(bytes) {
            Ok(_) => Ok(PublicKey { pk: bytes.clone() }),
            Err(e) => Err(e),
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.pk.clone()
    }

    pub fn verify(&self, signature: &[u8; 64], m: &[u8]) -> Result<(), Ed25519Error> {
        let r = Ed25519Point::decode(signature[..(B as usize) / 8].try_into().unwrap())?;
        let a = Ed25519Point::decode(&self.pk)?;
        let s = BigUint::from_bytes_le(&signature[(B as usize) / 8..]);
        let k = h_int(&[&r.encode()[..], &self.pk[..], m].concat());

        if (*BASE_POINT).clone().scalar_mul(s) != r + a.scalar_mul(k) {
            Err(Ed25519Error::Signature)
        } else {
            Ok(())
        }
    }
}

impl From<SecretKey> for PublicKey {
    fn from(sk: SecretKey) -> PublicKey {
        sk.public_key()
    }
}

impl From<PublicKey> for [u8; 32] {
    fn from(pk: PublicKey) -> [u8; 32] {
        pk.pk
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ed25519Point {
    x: BigUint,
    y: BigUint,
}

impl Ed25519Point {
    pub fn new(x: BigUint, y: BigUint) -> Result<Ed25519Point, Ed25519Error> {
        let p = Ed25519Point { x, y };
        if !p.is_on_curve() {
            Err(Ed25519Error::Point)
        } else {
            Ok(p)
        }
    }

    pub fn is_on_curve(&self) -> bool {
        let x = self.x.clone();
        let y = self.y.clone();

        // -x^2+y^2 = 1+dx^2y^2
        (((*Q).clone() - x.clone()) * x.clone() + y.clone() * y.clone()) % (*Q).clone()
            == (1u8 + (*D).clone() * x.clone() * x.clone() * y.clone() * y.clone()) % (*Q).clone()
    }

    pub fn scalar_mul(self, coef: BigUint) -> Ed25519Point {
        if coef == 0.to_biguint().unwrap() {
            Ed25519Point {
                x: 0.to_biguint().unwrap(),
                y: 1.to_biguint().unwrap(),
            }
        } else {
            let mut q = self.clone().scalar_mul(coef.clone() / 2u8);
            q = q.clone() + q;
            if coef.bit(0) {
                q = q + self;
            }
            q
        }
    }

    pub fn encode(&self) -> [u8; 32] {
        let mut n: BigUint = self.y.clone();
        n.set_bit(B - 1, self.x.bit(0));
        let mut r = n.to_bytes_le();
        r.resize(32, 0);
        r.try_into().unwrap()
    }

    pub fn decode(bytes: &[u8; 32]) -> Result<Ed25519Point, Ed25519Error> {
        let mut y = BigUint::from_bytes_le(bytes);
        let sign = y.bit(B - 1);
        y.set_bit(B - 1, false);

        let mut x = xrecover(&y);

        if x.bit(0) != sign {
            x = (*Q).clone() - x;
        }

        let p = Ed25519Point { x, y };

        if !p.is_on_curve() {
            Err(Ed25519Error::Point)
        } else {
            Ok(p)
        }
    }
}

impl Add for Ed25519Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let x1 = self.x;
        let y1 = self.y;
        let x2 = rhs.x;
        let y2 = rhs.y;

        // x3 = (x1*y2+x2*y1)/(1+d*x1*x2*y1*y2)
        let x3 = (x1.clone() * y2.clone() + x2.clone() * y1.clone())
            * inv(1.to_biguint().unwrap()
                + (*D).clone() * x1.clone() * x2.clone() * y1.clone() * y2.clone());
        // y3 = (y1*y2+x1*x2)/(1-d*x1*x2*y1*y2)
        let y3 = (y1.clone() * y2.clone() + x1.clone() * x2.clone())
            * inv(1.to_biguint().unwrap() + ((*Q).clone() - (*D).clone()) * x1 * x2 * y1 * y2);

        Ed25519Point {
            x: x3 % (*Q).clone(),
            y: y3 % (*Q).clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum Ed25519Error {
    #[error("Invalid point")]
    Point,
    #[error("Invalid signature")]
    Signature,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;

    #[test]
    fn test_keygen_sign_verify() {
        let sk = SecretKey::from_bytes(
            &hex::decode("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7")
                .unwrap()
                .try_into()
                .unwrap(),
        );
        //let sk = SecretKey::random();
        println!("sk:{:02x?}", sk.to_bytes());
        let pk: PublicKey = sk.public_key();
        println!("pk:{:02x?}", pk.to_bytes());
        let correct_pk: [u8; 32] =
            hex::decode("fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025")
                .unwrap()
                .try_into()
                .unwrap();
        println!("cr:{:02x?}", correct_pk);
        assert_eq!(pk.to_bytes(), correct_pk);
        let m:[u8;2] = [0xaf,0x82];
        let signature = sk.sign(&m);
        let correct_sig: [u8;64] =
            hex::decode("6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a")
                .unwrap()
                .try_into()
                .unwrap();
        assert_eq!(signature,correct_sig);
        println!("sig:{:02x?}", signature);
        assert!(pk.verify(&signature, &m).is_ok());
    }

    #[test]
    fn test_encdec() {
        let sk = SecretKey::random();
        let pk = sk.public_key();
        assert!(Ed25519Point::decode(&pk.pk).is_ok());
    }

    #[test]
    fn test_etc() {
        assert!(B >= 10);
        assert!(8 * h(b"hash input").len() == 2 * (B as usize));
        assert!(
            2.to_biguint()
                .unwrap()
                .modpow(&((*Q).clone() - 1u8), &(*Q).clone())
                == 1.to_biguint().unwrap()
        );
        assert!((*Q).clone() % 4u8 == 1.to_biguint().unwrap());
        assert!(
            2.to_biguint()
                .unwrap()
                .modpow(&((*L).clone() - 1u8), &(*L).clone())
                == 1.to_biguint().unwrap()
        );
        assert!((*L).clone() >= 2.to_biguint().unwrap().pow((B as u32) - 4));
        assert!((*L).clone() <= 2.to_biguint().unwrap().pow((B as u32) - 3));
        assert!(
            (*D).clone()
                .to_biguint()
                .unwrap()
                .modpow(&(((*Q).clone() - 1u8) / 2u8), &(*Q).clone())
                == (*Q).clone() - 1u8
        );
        assert!(
            (*I).clone()
                .to_biguint()
                .unwrap()
                .modpow(&2.to_biguint().unwrap(), &(*Q).clone())
                == (*Q).clone() - 1u8
        );
        assert!((*BASE_POINT).clone().is_on_curve());
        assert!(
            (*BASE_POINT).clone().scalar_mul((*L).clone())
                == Ed25519Point::new(0.to_biguint().unwrap(), 1.to_biguint().unwrap()).unwrap()
        );
    }

    #[test]
    fn test_masked() {
        let sk = SecretKey::random();
        println!("{:?}", sk);
    }
}
