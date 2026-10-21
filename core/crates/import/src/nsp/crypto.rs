//! The AES modes Switch content uses, on top of the `aes` block cipher: ECB for single key
//! blocks (key area, title key), XTS with Nintendo's tweak for NCA headers, CTR for sections.

use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Block};

/// Decrypt one 16-byte block (AES-128-ECB).
pub fn ecb_decrypt(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let mut b = Block::from(*block);
    Aes128::new(&Array::from(*key)).decrypt_block(&mut b);
    b.into()
}

fn xor16(dst: &mut [u8], src: &[u8; 16]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d ^= s;
    }
}

/// Multiply an XTS tweak by α in GF(2^128) (little-endian bit order, IEEE 1619).
fn next_tweak(t: &mut [u8; 16]) {
    let mut carry = 0u8;
    for b in t.iter_mut() {
        let c = *b >> 7;
        *b = (*b << 1) | carry;
        carry = c;
    }
    if carry != 0 {
        t[0] ^= 0x87;
    }
}

/// Decrypt one XTS data unit in place, given its tweak block (before encryption by key 2).
fn xts_decrypt_unit(k1: &Aes128, k2: &Aes128, tweak: [u8; 16], data: &mut [u8]) {
    let mut t = Block::from(tweak);
    k2.encrypt_block(&mut t);
    let mut t: [u8; 16] = t.into();
    let (blocks, _) = Block::slice_as_chunks_mut(data);
    let mut tweaks = Vec::with_capacity(blocks.len());
    for b in blocks.iter_mut() {
        xor16(b, &t);
        tweaks.push(t);
        next_tweak(&mut t);
    }
    k1.decrypt_blocks(blocks);
    for (b, t) in blocks.iter_mut().zip(&tweaks) {
        xor16(b, t);
    }
}

/// AES-128-XTS decryption with Nintendo's tweak (the sector number big-endian, where IEEE 1619
/// stores it little-endian), as NCA headers use. `key` is key 1 then key 2; `data` is a whole
/// number of `sector_size` sectors, the first numbered `first_sector`.
pub fn xts_decrypt_nintendo(
    key: &[u8; 32],
    sector_size: usize,
    first_sector: u64,
    data: &mut [u8],
) {
    let k1 = Aes128::new(&Array::from(<[u8; 16]>::try_from(&key[..16]).unwrap()));
    let k2 = Aes128::new(&Array::from(<[u8; 16]>::try_from(&key[16..]).unwrap()));
    for (i, sector) in data.chunks_mut(sector_size).enumerate() {
        let tweak = u128::from(first_sector + i as u64).to_be_bytes();
        xts_decrypt_unit(&k1, &k2, tweak, sector);
    }
}

/// The inverse of [`xts_decrypt_nintendo`] (to build test content).
#[cfg(test)]
pub(crate) fn xts_encrypt_nintendo(key: &[u8; 32], sector_size: usize, data: &mut [u8]) {
    let k1 = Aes128::new(&Array::from(<[u8; 16]>::try_from(&key[..16]).unwrap()));
    let k2 = Aes128::new(&Array::from(<[u8; 16]>::try_from(&key[16..]).unwrap()));
    for (i, sector) in data.chunks_mut(sector_size).enumerate() {
        let mut t = Block::from((i as u128).to_be_bytes());
        k2.encrypt_block(&mut t);
        let mut t: [u8; 16] = t.into();
        for b in sector.chunks_mut(16) {
            xor16(b, &t);
            let mut x = Block::try_from(&*b).unwrap();
            k1.encrypt_block(&mut x);
            b.copy_from_slice(&x);
            xor16(b, &t);
            next_tweak(&mut t);
        }
    }
}

#[cfg(test)]
pub(crate) fn ecb_encrypt(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let mut b = Block::from(*block);
    Aes128::new(&Array::from(*key)).encrypt_block(&mut b);
    b.into()
}

/// AES-128-CTR with a 128-bit big-endian counter.
pub struct Ctr(Aes128);

impl Ctr {
    pub fn new(key: &[u8; 16]) -> Self {
        Self(Aes128::new(&Array::from(*key)))
    }

    /// XOR `data` with the key stream starting at counter `iv` (encryption and decryption are
    /// the same). A partial last block uses the start of its key stream block.
    pub fn apply(&self, iv: u128, data: &mut [u8]) {
        const CHUNK: usize = 256;
        let mut stream = vec![Block::default(); CHUNK];
        let mut counter = iv;
        for chunk in data.chunks_mut(CHUNK * 16) {
            let n = chunk.len().div_ceil(16);
            for b in &mut stream[..n] {
                *b = Block::from(counter.to_be_bytes());
                counter = counter.wrapping_add(1);
            }
            self.0.encrypt_blocks(&mut stream[..n]);
            for (d, k) in chunk.chunks_mut(16).zip(&stream) {
                for (x, y) in d.iter_mut().zip(k.iter()) {
                    *x ^= y;
                }
            }
        }
    }
}

/// The counter Switch sections use for the byte at absolute NCA offset `offset`: the section's
/// upper 64 bits, then the offset in 16-byte blocks.
pub fn section_iv(upper: u64, offset: u64) -> u128 {
    (u128::from(upper) << 64) | u128::from(offset >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // IEEE 1619-2007 XTS-AES-128 vectors 1 and 2. Vector 1 is data unit 0 (the same either way
    // round); vector 2's unit number is given as the IEEE little-endian tweak block.
    #[test]
    fn xts_ieee1619() {
        let mut data = vec![0u8; 32];
        xts_decrypt_unit(
            &Aes128::new(&Array::from([0u8; 16])),
            &Aes128::new(&Array::from([0u8; 16])),
            [0; 16],
            &mut data,
        );
        let mut ct = hex("917cf69ebd68b2ec9b9fe9a3eadda692cd43d2f59598ed858c02c2652fbf922e");
        let mut key = [0u8; 32];
        xts_decrypt_nintendo(&key, 32, 0, &mut ct);
        assert_eq!(ct, vec![0u8; 32]);

        key[..16].fill(0x11);
        key[16..].fill(0x22);
        let mut ct = hex("c454185e6a16936e39334038acef838bfb186fff7480adc4289382ecd6d394f0");
        let mut tweak = [0u8; 16];
        tweak[..5].fill(0x33);
        xts_decrypt_unit(
            &Aes128::new(&Array::from([0x11u8; 16])),
            &Aes128::new(&Array::from([0x22u8; 16])),
            tweak,
            &mut ct,
        );
        assert_eq!(ct, vec![0x44u8; 32]);
        // Nintendo's tweak is the same unit number big-endian
        let mut a = hex("c454185e6a16936e39334038acef838bfb186fff7480adc4289382ecd6d394f0");
        let mut b = a.clone();
        xts_decrypt_nintendo(&key, 32, 0x33_3333_3333, &mut a);
        tweak = 0x33_3333_3333u128.to_be_bytes();
        xts_decrypt_unit(
            &Aes128::new(&Array::from([0x11u8; 16])),
            &Aes128::new(&Array::from([0x22u8; 16])),
            tweak,
            &mut b,
        );
        assert_eq!(a, b);
    }

    // NIST SP 800-38A F.5.1 CTR-AES128, including a partial block and a run across chunks.
    #[test]
    fn ctr_sp800_38a() {
        let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let iv = u128::from_be_bytes(hex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff").try_into().unwrap());
        let mut data = hex(
            "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
        );
        let ct = hex(
            "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee",
        );
        let ctr = Ctr::new(&key);
        ctr.apply(iv, &mut data);
        assert_eq!(data, ct);
        let mut part = ct[16..37].to_vec();
        ctr.apply(iv + 1, &mut part);
        assert_eq!(part, hex("ae2d8a571e03ac9c9eb76fac45af8e5130c81c46a3"));

        let mut long = vec![0u8; 16 * 600];
        ctr.apply(iv, &mut long);
        let mut tail = vec![0u8; 32];
        ctr.apply(iv + 300, &mut tail);
        assert_eq!(long[300 * 16..302 * 16], tail[..]);
    }
}
