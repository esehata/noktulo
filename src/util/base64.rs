const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn substitute(bits: u8) -> u8 {
    assert!(bits < 64);
    TABLE[bits as usize]
}

fn inv_substitute(c: u8) -> Result<u8, &'static str> {
    if c.is_ascii_alphanumeric() || c == b'+' || c == b'/' {
        Ok(TABLE.iter().position(|x| *x == c).unwrap() as u8)
    } else {
        Err("not a base64 character!")
    }
}

pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut s = Vec::new();

    if data.is_empty() {
        return s;
    }

    let pad_d = (6 - (data.len() * 8) % 6) % 6;

    let mut prev = 0;

    for (i, x) in data.iter().enumerate() {
        match i % 3 {
            0 => s.push(substitute(*x >> 2)),
            1 => s.push(substitute((prev & 0x03) << 4 | *x >> 4)),
            2 => {
                s.push(substitute((prev & 0x0F) << 2 | *x >> 6));
                s.push(substitute(*x & 0x3F));
            }
            _ => {}
        }
        prev = *x;
    }

    if pad_d > 0 {
        s.push(substitute(*data.last().unwrap() << pad_d & 0x3F));
    }

    let pad_s = (4 - s.len() % 4) % 4;

    for _ in 0..pad_s {
        s.push(b'=');
    }

    s
}

pub fn decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut v = Vec::new();

    if data.is_empty() {
        return Ok(v);
    }

    let mut prev = 0;

    for (i, c) in data.iter().enumerate() {
        if *c == b'=' {
            break;
        }

        let x = inv_substitute(*c)?;

        match i % 4 {
            0 => {
                prev = x << 2;
            }
            1 => {
                v.push(prev | x >> 4);
                prev = x << 4;
            }
            2 => {
                v.push(prev | x >> 2);
                prev = x << 6;
            }
            3 => {
                v.push(prev | x);
            }
            _ => {}
        }
    }

    Ok(v)
}

#[cfg(test)]
mod tests {
    use crate::util::base64::{decode, encode};

    #[test]
    fn base64_test() {
        let m = b"abcdefga";
        assert_eq!(
            String::from_utf8(m.to_vec()),
            String::from_utf8(decode(&encode(m)).unwrap())
        );
    }
}
