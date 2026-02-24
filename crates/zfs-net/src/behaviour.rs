use libp2p::swarm::NetworkBehaviour;

use crate::protocol::{ZfsRequest, ZfsResponse};

#[derive(NetworkBehaviour)]
pub(crate) struct ZfsBehaviour {
    pub(crate) gossipsub: libp2p::gossipsub::Behaviour,
    pub(crate) request_response: libp2p::request_response::cbor::Behaviour<ZfsRequest, ZfsResponse>,
    pub(crate) kademlia: libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
}
