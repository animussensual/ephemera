use serde::{Deserialize, Serialize};

use crate::{
    codec::{Decode, Encode, EphemeraEncoder},
    utilities::{
        crypto::Certificate,
        encoding::Encoder,
        encoding::{Decoder, EphemeraDecoder},
        hash::{EphemeraHash, EphemeraHasher},
        hash::{HashType, Hasher},
        time::EphemeraTime,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub(crate) struct EphemeraMessage {
    pub(crate) timestamp: u64,
    ///Application specific logical identifier of the message
    pub(crate) label: String,
    ///Application specific data
    pub(crate) data: Vec<u8>,
    ///Signature of the raw message
    pub(crate) certificate: Certificate,
}

impl EphemeraMessage {
    pub(crate) fn new(raw_message: RawEphemeraMessage, certificate: Certificate) -> Self {
        Self {
            timestamp: raw_message.timestamp,
            label: raw_message.label,
            data: raw_message.data,
            certificate,
        }
    }

    pub(crate) fn hash_with_default_hasher(&self) -> anyhow::Result<HashType> {
        let mut hasher = Hasher::default();
        self.hash(&mut hasher)?;
        let hash = hasher.finish().into();
        Ok(hash)
    }
}

impl Encode for EphemeraMessage {
    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Encoder::encode(&self)
    }
}

impl Decode for EphemeraMessage {
    type Output = Self;

    fn decode(bytes: &[u8]) -> anyhow::Result<Self::Output> {
        Decoder::decode(bytes)
    }
}

impl EphemeraHash for EphemeraMessage {
    fn hash<H: EphemeraHasher>(&self, state: &mut H) -> anyhow::Result<()> {
        state.update(&self.encode()?);
        Ok(())
    }
}

/// Raw message represents all the data what will be signed.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct RawEphemeraMessage {
    pub(crate) timestamp: u64,
    pub(crate) label: String,
    pub(crate) data: Vec<u8>,
}

impl RawEphemeraMessage {
    pub(crate) fn new(label: String, data: Vec<u8>) -> Self {
        Self {
            timestamp: EphemeraTime::now(),
            label,
            data,
        }
    }
}

impl From<EphemeraMessage> for RawEphemeraMessage {
    fn from(message: EphemeraMessage) -> Self {
        Self {
            timestamp: message.timestamp,
            label: message.label,
            data: message.data,
        }
    }
}

impl Encode for RawEphemeraMessage {
    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Encoder::encode(&self)
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::{EphemeraKeypair, EphemeraPublicKey, Keypair};

    use super::*;

    #[test]
    fn test_sign_ok() {
        let message_signing_keypair = Keypair::generate(None);

        let message =
            EphemeraMessage::signed("test".to_string(), vec![1, 2, 3], &message_signing_keypair)
                .unwrap();
        let certificate = message.certificate.clone();
        let raw_message: RawEphemeraMessage = message.into();
        let data = raw_message.encode().unwrap();

        assert!(certificate.public_key.verify(&data, &certificate.signature));
    }

    #[test]
    fn test_sign_fail() {
        let message_signing_keypair = Keypair::generate(None);

        let message =
            EphemeraMessage::signed("test".to_string(), vec![1, 2, 3], &message_signing_keypair)
                .unwrap();
        let certificate = message.certificate.clone();

        let modified_message = EphemeraMessage::signed(
            "test_test".to_string(),
            vec![1, 2, 3],
            &message_signing_keypair,
        )
        .unwrap();
        let raw_message: RawEphemeraMessage = modified_message.into();
        let data = raw_message.encode().unwrap();

        assert!(!certificate.public_key.verify(&data, &certificate.signature));
    }
}
