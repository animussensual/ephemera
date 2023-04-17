//! In Ephemera, membership of reliable broadcast protocol is decided by peer discovery.
//! Only peers who are returned by [crate::peer_discovery::PeerDiscovery] are allowed to participate.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use libp2p_identity::PeerId;
use lru::LruCache;

use crate::network::peer::Peer;

/// Peer discovery returns list of peers. But it is up to the Ephemera user to decide
/// how reliable the list is. For example, it can contain peers who are offline.

/// This enum defines how the actual membership is decided.
#[derive(Debug)]
pub(crate) enum MembershipKind {
    /// Specified threshold of peers(from total provided by [crate::peer_discovery::PeerDiscovery]) need to be available.
    /// Threshold value is defined the ratio of peers that need to be available.
    /// For example, if the threshold is 0.5, then at least 50% of the peers need to be available.
    Threshold(f64),
    /// Membership is defined by peers who are online.
    AnyOnline,
}

impl MembershipKind {
    pub(crate) fn accept(&self, connected_peers: usize, total_number_of_peers: usize) -> bool {
        match self {
            MembershipKind::Threshold(threshold) => {
                let minimum_available_nodes = (total_number_of_peers as f64 * threshold) as usize;
                connected_peers >= minimum_available_nodes
            }
            MembershipKind::AnyOnline => connected_peers > 0,
        }
    }
}

pub(crate) struct Memberships {
    snapshots: LruCache<u64, Membership>,
    current: u64,
    /// This is set when we get new peers set from [crate::peer_discovery::PeerDiscovery]
    /// but haven't yet activated it.
    pending_membership: Option<Membership>,
}

impl Memberships {
    pub(crate) fn new() -> Self {
        let mut snapshots = LruCache::new(NonZeroUsize::new(1000).unwrap());
        snapshots.put(0, Membership::new(Default::default()));
        Self {
            snapshots,
            current: 0,
            pending_membership: None,
        }
    }

    pub(crate) fn current(&mut self) -> &Membership {
        //Unwrap is safe because we always have current membership
        self.snapshots.get(&self.current).unwrap()
    }

    pub(crate) fn previous(&mut self) -> Option<&Membership> {
        self.snapshots.get(&(self.current - 1))
    }

    pub(crate) fn update(&mut self, membership: Membership) {
        self.current += 1;
        self.snapshots.put(self.current, membership);
    }

    pub(crate) fn set_pending(&mut self, membership: Membership) {
        self.pending_membership = Some(membership);
    }

    pub(crate) fn remove_pending(&mut self) -> Option<Membership> {
        self.pending_membership.take()
    }

    pub(crate) fn pending(&self) -> Option<&Membership> {
        self.pending_membership.as_ref()
    }

    pub(crate) fn pending_mut(&mut self) -> Option<&mut Membership> {
        self.pending_membership.as_mut()
    }
}

#[derive(Debug)]
pub(crate) struct Membership {
    local_peer_id: PeerId,
    all_members: HashMap<PeerId, Peer>,
    active_members: HashSet<PeerId>,
}

impl Membership {
    pub(crate) fn new_with_local(
        all_members: HashMap<PeerId, Peer>,
        local_peer_id: PeerId,
    ) -> Self {
        Self {
            local_peer_id,
            all_members,
            active_members: HashSet::new(),
        }
    }

    pub(crate) fn new(all_members: HashMap<PeerId, Peer>) -> Self {
        Self {
            local_peer_id: PeerId::random(),
            all_members,
            active_members: HashSet::new(),
        }
    }

    pub(crate) fn add_active_peer(&mut self, peer_id: PeerId) {
        self.active_members.insert(peer_id);
    }

    pub(crate) fn all_peer_ids_ref(&self) -> HashSet<&PeerId> {
        self.all_members.keys().collect()
    }

    pub(crate) fn active_peer_ids(&self) -> HashSet<PeerId> {
        self.active_members.clone()
    }

    pub(crate) fn active_peer_ids_with_local(&self) -> HashSet<PeerId> {
        let mut active_peers = self.active_members.clone();
        active_peers.insert(self.local_peer_id);
        active_peers
    }

    pub(crate) fn active_peer_ids_ref(&self) -> HashSet<&PeerId> {
        self.active_members.iter().collect()
    }

    pub(crate) fn peer_address(&self, peer_id: &PeerId) -> Option<&libp2p::Multiaddr> {
        self.all_members
            .get(peer_id)
            .map(|peer| peer.address.inner())
    }
}
