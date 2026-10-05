//! The digests that `cksum`, `sum` and the seven `*sum` tools all need.
//!
//! Eight tools want the same thing, so it is written once here:
//!
//! | tool | digest |
//! | --- | --- |
//! | `cksum` | all of these, selectable with `-a` |
//! | `cksum -a bsd`, `sum -r` | the BSD 16-bit checksum |
//! | `cksum -a sysv`, `sum -s` | the SysV byte sum |
//! | `cksum -a crc` | CRC-32/ISO-HDLC |
//! | `md5sum`, `cksum -a md5` | MD5 |
//! | `sha1sum`, `cksum -a sha1` | SHA-1 |
//! | `sha224sum` … `sha512sum` | SHA-224/256/384/512 |
//! | `b2sum`, `cksum -a blake2b` | BLAKE2b, at any digest length |
//! | `cksum -a sm3` | SM3 |
//!
//! Every one of these is a from-scratch implementation rather than a binding to a
//! library, so that the bytes on the wire are the bytes the specification says and
//! not whatever a dependency's version happens to produce. The unit tests are
//! against the published test vectors, and the differential tests check the
//! digests against the system tools end to end.
//!
//! Note that `sha2` carries its own constants rather than deriving them, because
//! the specification gives them as literals and deriving them has historically been
//! a source of quiet bugs.

mod blake2;
mod crc;
mod md5;
mod sha1;
mod sha2;
mod sm3;

pub use blake2::Blake2b;
pub use crc::{BsdSum, Crc32, SysvSum};
pub use md5::Md5;
pub use sha1::Sha1;
pub use sha2::{Sha224, Sha256, Sha384, Sha512};
pub use sm3::Sm3;

/// A digest that can be fed bytes and then asked for its value.
///
/// The trait exists so that `cksum` can hold one algorithm of each kind behind a
/// single type and hand chunks of the file to whichever is selected, without
/// knowing which it is. Implementors are all pure: no I/O, no allocation beyond
/// what the algorithm needs.
pub trait Hasher {
    /// Feed the next piece of the input.
    fn update(&mut self, bytes: &[u8]);

    /// The digest of everything fed so far, in bytes.
    ///
    /// `blake2b` is the one whose length is not fixed; it reports how many bytes
    /// it produced, which is also how `-l` is reflected in the output tag.
    ///
    /// This takes `&mut self` rather than consuming, because none of these need to
    /// own anything to finish. That leaves a provided one-shot below, which is what
    /// almost every caller actually wants. Asking twice gives the same answer
    /// twice, not a different one.
    fn finish(&mut self) -> Vec<u8>;

    /// The digest of everything fed so far, given how many bytes that was.
    ///
    /// Only one digest needs this: `cksum`'s CRC appends the file length to the
    /// checksum before complementing it, so a checksum that does not know the length
    /// cannot produce the number the tool prints. Everything else ignores it.
    fn finish_with_length(&mut self, length: u64) -> Vec<u8> {
        let _ = length;
        self.finish()
    }

    /// The digest of `bytes` on their own, which is what a caller that has the whole
    /// input in memory wants.
    fn digest(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.update(bytes);
        self.finish_with_length(bytes.len() as u64)
    }

    /// How many bytes `finish` will return, for the digest lengths that are known
    /// up front. BLAKE2b does not know until it is told, so it reports `None`.
    fn digest_len(&self) -> Option<usize> {
        None
    }
}

/// The digest for one of `cksum`'s `-a` values.
///
/// This is the table from `digest.c`: the name on the command line, the tag that
/// goes in front of a tagged digest, and the width in bits. `Blake2b` carries its
/// own width because that is the only one `-l` can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Bsd,
    Sysv,
    Crc,
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    /// The width in bits, which must be a multiple of 8 and at most 512.
    Blake2b(usize),
    Sm3,
}

/// The `-a` names, in the order `cksum` lists them when one is wrong.
pub const ALGORITHM_NAMES: [&str; 11] = [
    "bsd", "sysv", "crc", "md5", "sha1", "sha224", "sha256", "sha384", "sha512", "blake2b",
    "sm3",
];

impl Algorithm {
    /// Look an `-a` value up. Returns `None` for anything not in the table.
    pub fn from_name(name: &str) -> Option<Algorithm> {
        match name {
            "bsd" => Some(Algorithm::Bsd),
            "sysv" => Some(Algorithm::Sysv),
            "crc" => Some(Algorithm::Crc),
            "md5" => Some(Algorithm::Md5),
            "sha1" => Some(Algorithm::Sha1),
            "sha224" => Some(Algorithm::Sha224),
            "sha256" => Some(Algorithm::Sha256),
            "sha384" => Some(Algorithm::Sha384),
            "sha512" => Some(Algorithm::Sha512),
            "blake2b" => Some(Algorithm::Blake2b(512)),
            "sm3" => Some(Algorithm::Sm3),
            _ => None,
        }
    }

    /// The name `-a` was given, for the tag and for diagnostics.
    pub fn name(&self) -> &'static str {
        match self {
            Algorithm::Bsd => "bsd",
            Algorithm::Sysv => "sysv",
            Algorithm::Crc => "crc",
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha224 => "sha224",
            Algorithm::Sha256 => "sha256",
            Algorithm::Sha384 => "sha384",
            Algorithm::Sha512 => "sha512",
            Algorithm::Blake2b(_) => "blake2b",
            Algorithm::Sm3 => "sm3",
        }
    }

    /// The tag a tagged digest is introduced with.
    pub fn tag(&self) -> &'static str {
        match self {
            Algorithm::Bsd => "BSD",
            Algorithm::Sysv => "SYSV",
            Algorithm::Crc => "CRC",
            Algorithm::Md5 => "MD5",
            Algorithm::Sha1 => "SHA1",
            Algorithm::Sha224 => "SHA224",
            Algorithm::Sha256 => "SHA256",
            Algorithm::Sha384 => "SHA384",
            Algorithm::Sha512 => "SHA512",
            Algorithm::Blake2b(_) => "BLAKE2b",
            Algorithm::Sm3 => "SM3",
        }
    }

    /// The digest width in bits.
    pub fn bits(&self) -> usize {
        match self {
            Algorithm::Bsd | Algorithm::Sysv => 16,
            Algorithm::Crc => 32,
            Algorithm::Md5 => 128,
            Algorithm::Sha1 => 160,
            Algorithm::Sha224 => 224,
            Algorithm::Sha256 => 256,
            Algorithm::Sha384 => 384,
            Algorithm::Sha512 => 512,
            Algorithm::Blake2b(bits) => *bits,
            Algorithm::Sm3 => 256,
        }
    }

    /// Whether this algorithm's output is a number and a byte count rather than a
    /// digest.
    ///
    /// It decides three things at once, which is why they all hang off one flag:
    /// the numeric digests ignore `--base64`, they are never introduced by a tag
    /// even with `--tag`, and their `--untagged` shape is the same as their
    /// default one.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Algorithm::Bsd | Algorithm::Sysv | Algorithm::Crc)
    }

    /// A hasher for this algorithm, ready to be fed.
    pub fn hasher(&self) -> Box<dyn Hasher> {
        match self {
            Algorithm::Bsd => Box::new(BsdSum::default()),
            Algorithm::Sysv => Box::new(SysvSum::default()),
            Algorithm::Crc => Box::new(Crc32::default()),
            Algorithm::Md5 => Box::new(Md5::default()),
            Algorithm::Sha1 => Box::new(Sha1::default()),
            Algorithm::Sha224 => Box::new(Sha224::default()),
            Algorithm::Sha256 => Box::new(Sha256::default()),
            Algorithm::Sha384 => Box::new(Sha384::default()),
            Algorithm::Sha512 => Box::new(Sha512::default()),
            Algorithm::Blake2b(bits) => Box::new(Blake2b::new(*bits)),
            Algorithm::Sm3 => Box::new(Sm3::default()),
        }
    }

    /// The digest of `bytes`, for a one-shot caller.
    pub fn digest_of(&self, bytes: &[u8]) -> Vec<u8> {
        let mut hasher = self.hasher();
        hasher.update(bytes);
        hasher.finish_with_length(bytes.len() as u64)
    }
}

/// Lowercase hex, the form every one of these is printed in.
///
/// `cksum` and the `*sum` tools are the consumers, and every one of them wants
/// lowercase, so this is here rather than repeated eight times.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classic NIST vector input, used by nearly every digest below.
    const ABC: &[u8] = b"abc";
    const EMPTY: &[u8] = b"";
    const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog";

    #[test]
    fn hex_is_lowercase_and_two_digits_per_byte() {
        assert_eq!(to_hex(&[]), "");
        assert_eq!(to_hex(&[0x00]), "00");
        assert_eq!(to_hex(&[0x0f]), "0f");
        assert_eq!(to_hex(&[0xff, 0x10, 0x00]), "ff1000");
        assert_eq!(to_hex(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }

    #[test]
    fn the_algorithm_table_matches_the_source() {
        // The names, tags and widths, straight out of `digest.c`.
        let expected = [
            ("bsd", "BSD", 16usize, true),
            ("sysv", "SYSV", 16, true),
            ("crc", "CRC", 32, true),
            ("md5", "MD5", 128, false),
            ("sha1", "SHA1", 160, false),
            ("sha224", "SHA224", 224, false),
            ("sha256", "SHA256", 256, false),
            ("sha384", "SHA384", 384, false),
            ("sha512", "SHA512", 512, false),
            ("blake2b", "BLAKE2b", 512, false),
            ("sm3", "SM3", 256, false),
        ];
        assert_eq!(expected.len(), ALGORITHM_NAMES.len());
        for (name, tag, bits, numeric) in expected {
            let algorithm = Algorithm::from_name(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(algorithm.name(), name);
            assert_eq!(algorithm.tag(), tag, "tag for {name}");
            assert_eq!(algorithm.bits(), bits, "width for {name}");
            assert_eq!(algorithm.is_numeric(), numeric, "numeric for {name}");
        }
        assert!(Algorithm::from_name("b2a").is_none());
        assert!(Algorithm::from_name("sha").is_none());
        assert!(Algorithm::from_name("").is_none());
        // BLAKE2b is the one whose width `-l` sets, so it carries it.
        assert_eq!(Algorithm::from_name("blake2b"), Some(Algorithm::Blake2b(512)));
        assert_eq!(Algorithm::Blake2b(8).tag(), "BLAKE2b");
        assert_eq!(Algorithm::Blake2b(8).bits(), 8);
    }

    #[test]
    fn every_algorithms_digest_of_the_empty_input_is_its_published_value() {
        let expected = [
            (Algorithm::Md5, "d41d8cd98f00b204e9800998ecf8427e"),
            (Algorithm::Sha1, "da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            (
                Algorithm::Sha224,
                "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f",
            ),
            (
                Algorithm::Sha256,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                Algorithm::Sha384,
                "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da\
                 274edebfe76f65fbd51ad2f14898b95b",
            ),
            (
                Algorithm::Sha512,
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
                 47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            ),
            (
                Algorithm::Sm3,
                "1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b",
            ),
        ];
        for (algorithm, hex) in expected {
            assert_eq!(
                to_hex(&algorithm.digest_of(EMPTY)),
                hex,
                "empty input, {}",
                algorithm.name()
            );
        }
    }

    #[test]
    fn every_algorithms_digest_of_abc_is_its_published_value() {
        let expected = [
            (Algorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
            (Algorithm::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (
                Algorithm::Sha224,
                "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7",
            ),
            (
                Algorithm::Sha256,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                Algorithm::Sha384,
                "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed\
                 8086072ba1e7cc2358baeca134c825a7",
            ),
            (
                Algorithm::Sha512,
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
                 2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
            (
                Algorithm::Sm3,
                "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0",
            ),
        ];
        for (algorithm, hex) in expected {
            assert_eq!(
                to_hex(&algorithm.digest_of(ABC)),
                hex,
                "\"abc\", {}",
                algorithm.name()
            );
        }
    }

    #[test]
    fn every_algorithms_digest_of_the_quick_brown_fox_is_its_published_value() {
        // Checked against `cksum` as well, since these are the values the tools
        // have to reproduce byte for byte.
        let expected = [
            (Algorithm::Md5, "9e107d9d372bb6826bd81d3542a419d6"),
            (Algorithm::Sha1, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"),
            (
                Algorithm::Sha224,
                "730e109bd7a8a32b1cb9d9a09aa2325d2430587ddbc0c38bad911525",
            ),
            (
                Algorithm::Sha256,
                "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
            ),
            (
                Algorithm::Sha384,
                "ca737f1014a48f4c0b6dd43cb177b0afd9e5169367544c494011e3317dbf9a50\
                 9cb1e5dc1e85a941bbee3d7f2afbc9b1",
            ),
            (
                Algorithm::Sha512,
                "07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb64\
                 2e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6",
            ),
            (
                Algorithm::Sm3,
                "5fdfe814b8573ca021983970fc79b2218c9570369b4859684e2e4c3fc76cb8ea",
            ),
        ];
        for (algorithm, hex) in expected {
            assert_eq!(
                to_hex(&algorithm.digest_of(FOX)),
                hex,
                "the fox, {}",
                algorithm.name()
            );
        }
    }

    #[test]
    fn the_numeric_digests_are_the_numbers_the_tools_print() {
        // Read off `cksum`. "hello\n" is six bytes and the fox is forty-three.
        assert_eq!(to_hex(&Algorithm::Crc.digest_of(b"hello\n")), "b3beab91");
        assert_eq!(to_hex(&Algorithm::Crc.digest_of(FOX)), "7bab9ce8");
        assert_eq!(Algorithm::Sysv.digest_of(b"hello\n"), 542u16.to_be_bytes());
        assert_eq!(Algorithm::Sysv.digest_of(FOX), 4057u16.to_be_bytes());
        assert_eq!(Algorithm::Bsd.digest_of(b"hello\n"), 36979u16.to_be_bytes());
        assert_eq!(Algorithm::Bsd.digest_of(FOX), 50542u16.to_be_bytes());
    }

    #[test]
    fn blake2b_at_every_width_it_offers() {
        // The tag carries the width whenever it is not the 512-bit default, and
        // the digest is as long as the width says.
        for bits in [8usize, 16, 64, 128, 160, 256, 512] {
            let digest = Algorithm::Blake2b(bits).digest_of(FOX);
            assert_eq!(digest.len(), bits / 8, "width {bits}");
        }
        // The published 512-bit BLAKE2b vector for "abc".
        assert_eq!(
            to_hex(&Algorithm::Blake2b(512).digest_of(ABC)),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
             7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
    }

    #[test]
    fn the_helpers_work_out_of_the_box() {
        assert_eq!(to_hex(&Md5::default().digest(FOX)), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(to_hex(&Sha1::default().digest(FOX)), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
        assert_eq!(Md5::default().digest_len(), Some(16));
        assert_eq!(Sha512::default().digest_len(), Some(64));
        assert_eq!(Blake2b::new(256).digest_len(), Some(32));
        assert_eq!(Algorithm::Crc.digest_of(FOX).len(), 4);
        assert_eq!(Algorithm::Bsd.digest_of(FOX).len(), 2);
    }
}