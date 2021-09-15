use super::field::FiniteField;
use num_bigint::{BigUint, ToBigUint};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use sha3::{Digest, Sha3_512};
use std::convert::TryInto;
use std::ops::{Add, Sub};
use std::fmt;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct SecretKey {
    sk: [u8; 32],
}

impl SecretKey {
    pub fn from_bytes_le(bytes: [u8; 32]) -> SecretKey {
        SecretKey { sk: bytes }
    }

    pub fn from_bytes_be(bytes: [u8; 32]) -> SecretKey {
        let mut rev = bytes;
        rev.reverse();
        SecretKey { sk: rev }
    }

    pub fn random() -> SecretKey {
        let mut sk = [0; 32];
        ChaCha20Rng::from_entropy().fill_bytes(&mut sk);
        SecretKey { sk }
    }

    pub fn to_bytes_le(&self) -> [u8; 32] {
        self.sk
    }

    pub fn to_bytes_be(&self) -> [u8; 32] {
        let mut ret = self.sk;
        ret.reverse();
        ret
    }

    pub fn get_pubkey(&self) -> PublicKey {
        let h = Sha3_512::new().chain(self.sk).finalize();
        let mut a = BigUint::from_bytes_le(&h[..32]);
        a.set_bit(0, false);
        a.set_bit(1, false);
        a.set_bit(2, false);
        a.set_bit(254, true);
        a.set_bit(255, false);

        PublicKey {
            pk: Ed25519Point::bp().scalar_mul(a),
        }
    }

    pub fn sign<'a>(&self, message: &[u8]) -> Result<[u8; 64], &'a str> {
        let bp = Ed25519Point::bp();

        let h = Sha3_512::new().chain(self.sk).finalize();
        let b = [
            &(h[0] & 0xf8).to_le_bytes()[..],
            &h[1..31],
            &((h[31] & 0x7f) | 0x40).to_le_bytes()[..],
        ]
        .concat();
        let s = BigUint::from_bytes_le(&b[..]);
        let sb = bp.clone().scalar_mul(s.clone());
        let prefix = &h[32..];
        let r = BigUint::from_bytes_le(
            &Sha3_512::new().chain([prefix, message].concat()).finalize()[..],
        );
        let rb = bp.scalar_mul(r.clone());
        let k = BigUint::from_bytes_le(
            &Sha3_512::new()
                .chain([&rb.encode(), &sb.encode(), message].concat())
                .finalize()[..],
        );
        let v = (r + k * s) % Ed25519Point::l();
        let mut _s = v.to_bytes_le();
        _s.resize(32, 0);
        Ok([rb.encode(), _s.try_into().unwrap()]
            .concat()
            .try_into()
            .unwrap())
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(),fmt::Error> {
        let masked = format!("This data doesn't display for privacy");
        f.debug_struct("SecretKey").field("sk",&masked).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicKey {
    pk: Ed25519Point,
}

impl PublicKey {
    pub fn from_bytes_le(bytes: &[u8; 32]) -> PublicKey {
        PublicKey {
            pk: Ed25519Point::decode(&bytes).unwrap(),
        }
    }

    pub fn from_bytes_be(mut bytes: [u8; 32]) -> PublicKey {
        bytes.reverse();
        PublicKey {
            pk: Ed25519Point::decode(&bytes).unwrap(),
        }
    }

    pub fn to_bytes_le(&self) -> [u8; 32] {
        self.pk.encode()
    }

    pub fn to_bytes_be(&self) -> [u8; 32] {
        let mut ret = self.pk.encode();
        ret.reverse();
        ret
    }

    pub fn verify(&self, signature: &[u8; 64], m: &[u8]) -> Result<bool, &str> {
        let r = Ed25519Point::decode(signature[..32].try_into().unwrap())?;
        let s = BigUint::from_bytes_le(&signature[32..]);
        let h = BigUint::from_bytes_le(
            &Sha3_512::new()
                .chain([&r.encode(), &self.pk.encode(), m].concat())
                .finalize()[..],
        );

        let _8 = 8.to_biguint().unwrap();

        // 8*s*b = 8*r + 8*h*a
        Ok(Ed25519Point::bp().scalar_mul(_8.clone() * s)
            == r.scalar_mul(_8.clone()) + self.pk.clone().scalar_mul(_8.clone() * h))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Ed25519Point {
    x: FiniteField,
    y: FiniteField,
}

impl Ed25519Point {
    pub fn new(x: BigUint, y: BigUint) -> Ed25519Point {
        let q = Ed25519Point::q();

        let x = FiniteField::new(x, q.clone());
        let y = FiniteField::new(y, q.clone());

        assert!(Ed25519Point::is_on_curve(x.clone(), y.clone()));

        Ed25519Point { x, y }
    }

    pub fn is_on_curve(x: FiniteField, y: FiniteField) -> bool {
        let q = Ed25519Point::q();
        let d = Ed25519Point::d();

        let _1 = FiniteField::new(1.to_biguint().unwrap(), q.clone());
        let _2 = 2.to_biguint().unwrap();

        // -x^2+y^2 = 1+dx^2y^2
        -x.clone().pow(&_2) + y.clone().pow(&_2) == _1 + d * x.pow(&_2) * y.pow(&_2)
    }

    pub fn scalar_mul(self, coef: BigUint) -> Ed25519Point {
        let mut coef = coef % Ed25519Point::l();
        let mut current = self.clone();
        let mut result = Ed25519Point::new(0.to_biguint().unwrap(), 1.to_biguint().unwrap());

        while coef > 0.to_biguint().unwrap() {
            if coef.bit(0) {
                result = result + current.clone();
            }
            current = current.clone() + current;
            coef = coef >> 1;
        }

        result
    }

    pub fn encode(&self) -> [u8; 32] {
        let mut n: BigUint = self.y.num().clone();
        n.set_bit(255, self.x.num().bit(0));
        n.to_bytes_le().try_into().unwrap()
    }

    pub fn decode(bytes: &[u8; 32]) -> Result<Ed25519Point, &'static str> {
        // constants
        let q = Ed25519Point::q();
        let d = Ed25519Point::d();
        let _0 = FiniteField::new(0.to_biguint().unwrap(), q.clone());
        let _1 = FiniteField::new(1.to_biguint().unwrap(), q.clone());
        let _2 = FiniteField::new(2.to_biguint().unwrap(), q.clone());

        let mut y = BigUint::from_bytes_le(bytes);
        let sign = y.bit(255);
        y.set_bit(255, false);
        let y = FiniteField::new(y, q.clone());

        // x^2 = (y^2-1)/(dy^2+1)
        let x2 =
            (y.clone() * y.clone() - _1.clone()) / (d.clone() * y.clone() * y.clone() + _1.clone());

        if x2 == _0 {
            if sign {
                return Err("invalid sign of encode point");
            }
        }

        // x = +- sqrt((y^2-1)/(dy^2+1))
        let mut x = x2.clone().pow(&((q.clone() + 3u8) / 8u8)); // x = x2^((q+3)/8)
        if (x.clone().pow(_2.num()) - x2.clone()) != _0 {
            // x^2 - x2 != 0
            x = x.clone() * _2.pow(&((q.clone() - 1u8) / 4u8)); // x = x*2^((q-1)/4)
            if (x.clone().pow(_2.num()) - x2) != _0 {
                // x^2 - x2 != 0
                return Err("invalid square of encode point");
            }
        }
        if x.num().bit(0) != sign {
            x = -x;
        }
        Ok(Ed25519Point { x, y })
    }

    /// prime order
    pub fn q() -> BigUint {
        BigUint::parse_bytes(
            b"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffed",
            16,
        )
        .unwrap()
    }

    /// non-quadratic element of F_q
    pub fn d() -> FiniteField {
        FiniteField::new(
            BigUint::parse_bytes(
                b"52036cee2b6ffe738cc740797779e89800700a4d4141d8ab75eb4dca135978a3",
                16,
            )
            .unwrap(),
            Ed25519Point::q(),
        )
    }

    pub fn l() -> BigUint {
        BigUint::parse_bytes(
            b"1000000000000000000000000000000014def9dea2f79cd65812631a5cf5d3ed",
            16,
        )
        .unwrap()
    }

    /// basepoint
    pub fn bp() -> Ed25519Point {
        let q = Ed25519Point::q();
        let _4 = FiniteField::new(4.to_biguint().unwrap(), q.clone());
        let _5 = FiniteField::new(5.to_biguint().unwrap(), q.clone());
        let y = _4 / _5;
        Ed25519Point::decode(&y.num().to_bytes_le().try_into().unwrap()).unwrap()
    }
}

impl Add for Ed25519Point {
    type Output = Self;
    fn add(self, rhs: Ed25519Point) -> Ed25519Point {
        let q = Ed25519Point::q();
        let d = Ed25519Point::d();
        let x1 = self.x;
        let y1 = self.y;
        let x2 = rhs.x;
        let y2 = rhs.y;

        let _1 = FiniteField::new(1.to_biguint().unwrap(), q.clone());

        // x3 = (x1y2+x2y1)/(1+dx1x2y1y2)
        let x3 = (x1.clone() * y2.clone() + x2.clone() * y1.clone())
            / (_1.clone() + d.clone() * x1.clone() * x2.clone() * y1.clone() * y2.clone());
        // y3 = (y1y2+x1x2)/(1-dx1x2y1y2)
        let y3 = (y1.clone() * y2.clone() + x1.clone() * x2.clone())
            / (_1 - d.clone() * x1 * x2 * y1 * y2);

        Ed25519Point { x: x3, y: y3 }
    }
}

impl Sub for Ed25519Point {
    type Output = Self;
    fn sub(self, rhs: Ed25519Point) -> Ed25519Point {
        let q = Ed25519Point::q();
        self + Ed25519Point::new(q - rhs.x.num(), rhs.y.num().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_d() {
        let q = Ed25519Point::q();
        let d = -FiniteField::new(121665.to_biguint().unwrap(), q.clone())
            / FiniteField::new(121666.to_biguint().unwrap(), q.clone());
        assert_eq!(Ed25519Point::d(), d);
    }

    #[test]
    fn test_keygen_sign_verify() {
        let sk = SecretKey::random();
        let pk = sk.get_pubkey();
        let m = "hello";
        let signature = sk.sign(m.as_bytes()).unwrap();
        assert!(pk
            .verify(&signature.try_into().unwrap(), m.as_bytes())
            .unwrap());
    }

    #[test]
    fn test_masked() {
        let sk = SecretKey::random();
        println!("{:?}", sk);
    }
}
