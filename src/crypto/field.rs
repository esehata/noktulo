use num_bigint::BigUint;
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FiniteField {
    num: BigUint,
    prime: BigUint,
}

impl FiniteField {
    pub fn new(num: BigUint, prime: BigUint) -> FiniteField {
        FiniteField {
            num: num % prime.clone(),
            prime,
        }
    }

    pub fn pow(&self, exponent: &BigUint) -> FiniteField {
        let num = self.num.modpow(exponent, &self.prime);
        FiniteField {
            num,
            prime: self.prime.clone(),
        }
    }
    
    pub fn num(&self) -> &BigUint {
        &self.num
    }
}

impl Add for FiniteField {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        assert_eq!(self.prime, rhs.prime);
        let num = (self.num + rhs.num) % self.prime.clone();
        FiniteField {
            num,
            prime: self.prime,
        }
    }
}

impl Sub for FiniteField {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        assert_eq!(self.prime, rhs.prime);
        let num = (self.prime.clone() + self.num - rhs.num) % self.prime.clone();
        FiniteField {
            num,
            prime: self.prime,
        }
    }
}

impl Mul for FiniteField {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        assert_eq!(self.prime, rhs.prime);
        let num = (self.num * rhs.num) % self.prime.clone();
        FiniteField {
            num,
            prime: self.prime,
        }
    }
}

impl Div for FiniteField {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        assert_eq!(self.prime, rhs.prime);
        assert_ne!(rhs.num, BigUint::from(0u8));
        let num = (self.num * rhs.num.modpow(&(self.prime.clone() - 2u8), &self.prime)) % self.prime.clone();
        FiniteField {
            num,
            prime: self.prime,
        }
    }
}

impl Neg for FiniteField {
    type Output = Self;
    fn neg(self) -> Self {
        FiniteField {
            num: self.prime.clone() - self.num,
            prime: self.prime,
        }
    }
}
