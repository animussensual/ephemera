use std::fmt::Display;

use libp2p::PeerId as Libp2pPeerId;
use serde::{Deserialize, Serialize};

pub(crate) type PeerIdType = Libp2pPeerId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PeerId(pub(crate) PeerIdType);

impl PeerId {
    pub fn random() -> Self {
        Self(PeerIdType::random())
    }
}

impl From<PeerId> for libp2p_identity::PeerId {
    fn from(peer_id: PeerId) -> Self {
        peer_id.0
    }
}

impl Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait ToPeerId {
    fn peer_id(&self) -> PeerId;
}
