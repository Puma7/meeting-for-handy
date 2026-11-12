use ring::{
    aead::{
        self, Aad, BoundKey, LessSafeKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey,
    },
    hkdf, pbkdf2,
    rand::{generate, SecureRandom, SystemRandom},
};
use std::{
    io::{self, Read, Write},
    num::NonZeroU32,
};
use zeroize::{Zeroize, Zeroizing};

use super::{RecoveryKey, Unlock};

const V1: &[u8; 8] = b"HMSAFE01";
const V2: &[u8; 8] = b"HMSAFE02";
const SALT_LEN: usize = 32;
const PREFIX_LEN: usize = V1.len() + SALT_LEN;
const SLOT_LEN: usize = 32 + TAG;
pub(super) const HEADER_LEN: usize = PREFIX_LEN + 2 * SLOT_LEN;
pub(super) const BLOCK: usize = 64 * 1024;
const TAG: usize = 16;
const ITERATIONS: NonZeroU32 = NonZeroU32::new(600_000).unwrap();

struct Counter(u64);

impl NonceSequence for Counter {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let current = self.0;
        // A stream is bounded well below 2^32 frames.
        self.0 = current
            .checked_add(1)
            .filter(|next| *next <= u32::MAX.into())
            .ok_or(ring::error::Unspecified)?;
        let mut nonce = [0; 12];
        nonce[4..].copy_from_slice(&current.to_be_bytes());
        Ok(Nonce::assume_unique_for_key(nonce))
    }
}

fn format_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Passphrase oder Wiederherstellungsschlüssel falsch, oder Sicherung beschädigt. / Wrong passphrase or recovery key, or damaged encrypted backup.",
    )
}

fn derive(salt: &[u8], password: &[u8]) -> io::Result<UnboundKey> {
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        ITERATIONS,
        salt,
        password,
        &mut *key,
    );
    UnboundKey::new(&aead::AES_256_GCM, &*key).map_err(|_| format_error())
}

fn recovery_wrap_key(salt: &[u8], recovery: &RecoveryKey) -> io::Result<UnboundKey> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&*recovery.0);
    let context = [b"HandyMeeting backup recovery key wrap v2".as_slice()];
    let mut key = Zeroizing::new([0u8; 32]);
    prk.expand(&context, &aead::AES_256_GCM)
        .and_then(|okm| okm.fill(&mut *key))
        .map_err(|_| format_error())?;
    UnboundKey::new(&aead::AES_256_GCM, &*key).map_err(|_| format_error())
}

fn slot_aad(prefix: &[u8], slot: u8) -> Aad<[u8; PREFIX_LEN + 1]> {
    let mut bytes = [0u8; PREFIX_LEN + 1];
    bytes[..PREFIX_LEN].copy_from_slice(prefix);
    bytes[PREFIX_LEN] = slot;
    Aad::from(bytes)
}

fn wrap(
    key: UnboundKey,
    prefix: &[u8],
    slot: u8,
    data_key: &[u8; 32],
) -> io::Result<[u8; SLOT_LEN]> {
    let mut wrapped = Zeroizing::new([0u8; SLOT_LEN]);
    wrapped[..32].copy_from_slice(data_key);
    // Each wrapping key is derived with a fresh file salt and used for one slot.
    let tag = LessSafeKey::new(key)
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key([0; 12]),
            slot_aad(prefix, slot),
            &mut wrapped[..32],
        )
        .map_err(|_| format_error())?;
    wrapped[32..].copy_from_slice(tag.as_ref());
    Ok(*wrapped)
}

fn frame_aad(header: &[u8], length: [u8; 4], bytes: &mut [u8; HEADER_LEN + 4]) {
    bytes[..header.len()].copy_from_slice(header);
    bytes[header.len()..header.len() + 4].copy_from_slice(&length);
}

fn random(bytes: &mut [u8]) -> io::Result<()> {
    SystemRandom::new()
        .fill(bytes)
        .map_err(|_| io::Error::other("Could not generate backup encryption material"))
}

fn write_header(
    sink: &mut impl Write,
    password: &[u8],
    recovery: &RecoveryKey,
) -> io::Result<([u8; HEADER_LEN], UnboundKey)> {
    let salt = generate::<[u8; SALT_LEN]>(&SystemRandom::new())
        .map_err(|_| io::Error::other("Could not generate backup encryption material"))?
        .expose();
    let mut header = [0u8; HEADER_LEN];
    header[..V2.len()].copy_from_slice(V2);
    header[V2.len()..PREFIX_LEN].copy_from_slice(&salt);
    let prefix = &header[..PREFIX_LEN];
    let mut data_key = Zeroizing::new([0u8; 32]);
    random(&mut *data_key)?;
    let password_slot = wrap(derive(&salt, password)?, prefix, 0, &data_key)?;
    let recovery_slot = wrap(recovery_wrap_key(&salt, recovery)?, prefix, 1, &data_key)?;
    header[PREFIX_LEN..PREFIX_LEN + SLOT_LEN].copy_from_slice(&password_slot);
    header[PREFIX_LEN + SLOT_LEN..].copy_from_slice(&recovery_slot);
    let key = UnboundKey::new(&aead::AES_256_GCM, &*data_key).map_err(|_| format_error())?;
    sink.write_all(&header)?;
    Ok((header, key))
}

fn open_header(
    source: &mut impl Read,
    unlock: Unlock<'_>,
) -> io::Result<([u8; HEADER_LEN], usize, UnboundKey)> {
    let mut prefix = Vec::with_capacity(PREFIX_LEN);
    (&mut *source)
        .take(PREFIX_LEN as u64)
        .read_to_end(&mut prefix)
        .map_err(|_| format_error())?;
    let prefix: [u8; PREFIX_LEN] = prefix.try_into().map_err(|_| format_error())?;
    let salt = &prefix[V1.len()..];
    let mut header = [0u8; HEADER_LEN];
    header[..PREFIX_LEN].copy_from_slice(&prefix);
    match &prefix[..V1.len()] {
        magic if magic == V1 => {
            let Unlock::Passphrase(password) = unlock else {
                return Err(format_error());
            };
            let key = derive(salt, password)?;
            Ok((header, PREFIX_LEN, key))
        }
        magic if magic == V2 => {
            let (slot, key) = match unlock {
                Unlock::Passphrase(password) => (0, derive(salt, password)?),
                Unlock::RecoveryKey(recovery) => (1, recovery_wrap_key(salt, recovery)?),
            };
            source
                .read_exact(&mut header[PREFIX_LEN..])
                .map_err(|_| format_error())?;
            let start = PREFIX_LEN + usize::from(slot) * SLOT_LEN;
            let mut wrapped = Zeroizing::new([0u8; SLOT_LEN]);
            wrapped.copy_from_slice(&header[start..start + SLOT_LEN]);
            let data_key = LessSafeKey::new(key)
                .open_in_place(
                    Nonce::assume_unique_for_key([0; 12]),
                    slot_aad(&header[..PREFIX_LEN], slot),
                    &mut *wrapped,
                )
                .map_err(|_| format_error())?;
            let key = UnboundKey::new(&aead::AES_256_GCM, data_key).map_err(|_| format_error())?;
            Ok((header, HEADER_LEN, key))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unbekanntes Sicherungsformat. / Unsupported encrypted backup format.",
        )),
    }
}

impl RecoveryKey {
    pub(super) fn generate() -> io::Result<Self> {
        let mut key = Zeroizing::new([0u8; 32]);
        random(&mut *key)?;
        Ok(Self(key))
    }
}

pub(super) struct Encrypt<W> {
    sink: W,
    header: [u8; HEADER_LEN],
    key: SealingKey<Counter>,
    frame: Zeroizing<Vec<u8>>,
}

impl<W: Write> Encrypt<W> {
    pub(super) fn new(mut sink: W, password: &[u8], recovery: &RecoveryKey) -> io::Result<Self> {
        let (header, key) = write_header(&mut sink, password, recovery)?;
        Ok(Self {
            sink,
            header,
            key: SealingKey::new(key, Counter(0)),
            frame: Zeroizing::new(Vec::with_capacity(BLOCK + TAG)),
        })
    }

    fn seal_frame(&mut self) -> io::Result<()> {
        let length = (self.frame.len() as u32).to_be_bytes();
        let mut associated = [0u8; HEADER_LEN + 4];
        frame_aad(&self.header, length, &mut associated);
        self.key
            .seal_in_place_append_tag(Aad::from(associated), &mut *self.frame)
            .map_err(|_| format_error())?;
        self.sink.write_all(&length)?;
        self.sink.write_all(&self.frame)?;
        self.frame.as_mut_slice().zeroize();
        self.frame.clear();
        Ok(())
    }

    pub(super) fn finish(mut self) -> io::Result<()> {
        if !self.frame.is_empty() {
            self.seal_frame()?;
        }
        self.seal_frame()?;
        self.sink.flush()
    }
}

impl<W: Write> Write for Encrypt<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let count = data.len().min(BLOCK - self.frame.len());
        self.frame.extend_from_slice(&data[..count]);
        if self.frame.len() == BLOCK {
            self.seal_frame()?;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

pub(super) struct Decrypt<R> {
    source: R,
    header: [u8; HEADER_LEN],
    header_len: usize,
    key: OpeningKey<Counter>,
    frame: Zeroizing<Vec<u8>>,
    position: usize,
    short: bool,
    ended: bool,
}

impl<R: Read> Decrypt<R> {
    pub(super) fn new(mut source: R, unlock: Unlock<'_>) -> io::Result<Self> {
        let (header, header_len, key) = open_header(&mut source, unlock)?;
        Ok(Self {
            source,
            header,
            header_len,
            key: OpeningKey::new(key, Counter(0)),
            frame: Zeroizing::new(Vec::with_capacity(BLOCK + TAG)),
            position: 0,
            short: false,
            ended: false,
        })
    }

    fn next_frame(&mut self) -> io::Result<()> {
        let mut length = [0u8; 4];
        self.source
            .read_exact(&mut length)
            .map_err(|_| format_error())?;
        let count = u32::from_be_bytes(length) as usize;
        if count > BLOCK || (self.short && count != 0) {
            return Err(format_error());
        }
        self.frame.as_mut_slice().zeroize();
        self.frame.resize(count + TAG, 0);
        self.source
            .read_exact(&mut self.frame)
            .map_err(|_| format_error())?;
        let mut associated = [0u8; HEADER_LEN + 4];
        frame_aad(&self.header[..self.header_len], length, &mut associated);
        self.key
            .open_in_place(
                Aad::from(&associated[..self.header_len + 4]),
                &mut self.frame,
            )
            .map_err(|_| format_error())?;
        self.frame.truncate(count);
        self.position = 0;
        self.short = count < BLOCK;
        if count == 0 {
            let mut trailing = [0u8; 1];
            if self.source.read(&mut trailing)? != 0 {
                return Err(format_error());
            }
            self.ended = true;
        }
        Ok(())
    }
}

impl<R: Read> Read for Decrypt<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.ended {
            return Ok(0);
        }
        if self.position == self.frame.len() {
            self.next_frame()?;
        }
        let count = out.len().min(self.frame.len() - self.position);
        out[..count].copy_from_slice(&self.frame[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
