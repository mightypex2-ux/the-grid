use grid_core::{SectorRequest, SectorResponse};
use libp2p::swarm::NetworkBehaviour;

#[derive(NetworkBehaviour)]
pub(crate) struct GridBehaviour {
    pub(crate) gossipsub: libp2p::gossipsub::Behaviour,
    pub(crate) sector_rr: libp2p::request_response::cbor::Behaviour<SectorRequest, SectorResponse>,
    pub(crate) kademlia: libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
    pub(crate) relay: libp2p::relay::client::Behaviour,
    pub(crate) identify: libp2p::identify::Behaviour,
    pub(crate) ping: libp2p::ping::Behaviour,
}
