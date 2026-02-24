use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    gossipsub, kad, request_response, swarm::SwarmEvent, Multiaddr, PeerId, StreamProtocol,
};
use tracing::{debug, info};

use crate::behaviour::{ZfsBehaviour, ZfsBehaviourEvent};
use crate::config::{KademliaMode, NetworkConfig};
use crate::error::NetworkError;
use crate::event::NetworkEvent;
use crate::protocol::{ZfsRequest, ZfsResponse};

const ZFS_PROTOCOL: &str = "/zfs/1.0.0";
const ZFS_KAD_PROTOCOL: &str = "/zfs/kad/1.0.0";

/// Manages the libp2p swarm and exposes the ZFS network API.
///
/// The caller drives the service by calling [`next_event`](Self::next_event)
/// in a loop, handling events and issuing commands via the other methods.
pub struct NetworkService {
    swarm: libp2p::Swarm<ZfsBehaviour>,
    kademlia_enabled: bool,
    random_walk_interval: Duration,
    max_discovery_dials: usize,
    pending_discovery_dials: usize,
    discovered_peers: HashSet<PeerId>,
}

impl NetworkService {
    /// Create and start the network service.
    ///
    /// Begins listening on `config.listen_addr` and dials any bootstrap peers.
    /// When `config.discovery.enable_kademlia` is true, seeds the Kademlia
    /// routing table and triggers the initial bootstrap.
    pub async fn new(config: NetworkConfig) -> Result<Self, NetworkError> {
        let kademlia_enabled = config.discovery.enable_kademlia;
        let random_walk_interval = config.discovery.random_walk_interval;
        let max_discovery_dials = config.discovery.max_concurrent_discovery_dials;
        let kademlia_mode = config.discovery.kademlia_mode;

        let mut swarm = build_swarm()?;

        if kademlia_enabled {
            let mode = match kademlia_mode {
                KademliaMode::Server => kad::Mode::Server,
                KademliaMode::Client => kad::Mode::Client,
            };
            swarm.behaviour_mut().kademlia.set_mode(Some(mode));
        }

        swarm
            .listen_on(config.listen_addr)
            .map_err(|e| NetworkError::Transport(e.to_string()))?;

        dial_bootstrap_peers(&mut swarm, &config.bootstrap_peers, kademlia_enabled)?;

        if kademlia_enabled {
            if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                debug!("kademlia bootstrap not started (need at least one peer): {e:?}");
            }
        }

        Ok(Self {
            swarm,
            kademlia_enabled,
            random_walk_interval,
            max_discovery_dials,
            pending_discovery_dials: 0,
            discovered_peers: HashSet::new(),
        })
    }

    /// The local Zode ID.
    pub fn local_zode_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    /// Subscribe to a GossipSub topic (e.g. `"prog/{program_id_hex}"`).
    pub fn subscribe(&mut self, topic: &str) -> Result<(), NetworkError> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&topic)
            .map_err(|e| NetworkError::Subscription(e.to_string()))?;
        Ok(())
    }

    /// Unsubscribe from a GossipSub topic.
    pub fn unsubscribe(&mut self, topic: &str) -> Result<(), NetworkError> {
        let topic = gossipsub::IdentTopic::new(topic);
        if !self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic) {
            return Err(NetworkError::Subscription("not subscribed to topic".into()));
        }
        Ok(())
    }

    /// Publish data to a GossipSub topic.
    pub fn publish(&mut self, topic: &str, data: Vec<u8>) -> Result<(), NetworkError> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map_err(|e| NetworkError::Publish(e.to_string()))?;
        Ok(())
    }

    /// Send a store request to a specific peer. Returns the outbound request ID.
    pub fn send_store(
        &mut self,
        peer: &PeerId,
        request: zfs_core::StoreRequest,
    ) -> request_response::OutboundRequestId {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_request(peer, ZfsRequest::Store(Box::new(request)))
    }

    /// Send a fetch request to a specific peer. Returns the outbound request ID.
    pub fn send_fetch(
        &mut self,
        peer: &PeerId,
        request: zfs_core::FetchRequest,
    ) -> request_response::OutboundRequestId {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_request(peer, ZfsRequest::Fetch(request))
    }

    /// Send a store response on a previously received request channel.
    pub fn send_store_response(
        &mut self,
        channel: request_response::ResponseChannel<ZfsResponse>,
        response: zfs_core::StoreResponse,
    ) -> Result<(), NetworkError> {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, ZfsResponse::Store(response))
            .map_err(|_| NetworkError::ResponseFailed)
    }

    /// Send a fetch response on a previously received request channel.
    pub fn send_fetch_response(
        &mut self,
        channel: request_response::ResponseChannel<ZfsResponse>,
        response: zfs_core::FetchResponse,
    ) -> Result<(), NetworkError> {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, ZfsResponse::Fetch(Box::new(response)))
            .map_err(|_| NetworkError::ResponseFailed)
    }

    /// Returns the list of currently connected peer IDs.
    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.swarm.connected_peers().copied().collect()
    }

    /// Dial a peer at the given multiaddr.
    pub fn dial(&mut self, addr: Multiaddr) -> Result<(), NetworkError> {
        self.swarm
            .dial(addr)
            .map_err(|e| NetworkError::Dial(e.to_string()))
    }

    /// Drive the swarm event loop, returning the next high-level network event.
    ///
    /// Must be called in a loop to keep the network alive. When Kademlia is
    /// enabled, a periodic random walk timer fires between swarm events.
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        let sleep = tokio::time::sleep(self.random_walk_interval);
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    if let Some(net_event) = self.handle_swarm_event(event) {
                        return Some(net_event);
                    }
                }
                () = &mut sleep, if self.kademlia_enabled => {
                    self.trigger_random_walk();
                    sleep.as_mut().reset(tokio::time::Instant::now() + self.random_walk_interval);
                }
            }
        }
    }

    fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<ZfsBehaviourEvent>,
    ) -> Option<NetworkEvent> {
        match event {
            SwarmEvent::Behaviour(event) => self.map_behaviour_event(event),
            SwarmEvent::ConnectionEstablished {
                peer_id,
                num_established,
                endpoint,
                ..
            } => {
                if self.pending_discovery_dials > 0 {
                    self.pending_discovery_dials -= 1;
                }
                debug!(%peer_id, num = %num_established, "connection established");
                if self.kademlia_enabled {
                    let addr = endpoint.get_remote_address().clone();
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                }
                (num_established.get() == 1).then(|| NetworkEvent::PeerConnected(peer_id))
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                num_established,
                ..
            } => {
                if self.pending_discovery_dials > 0 {
                    self.pending_discovery_dials -= 1;
                }
                debug!(%peer_id, num = %num_established, "connection closed");
                (num_established == 0).then(|| NetworkEvent::PeerDisconnected(peer_id))
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "listening");
                Some(NetworkEvent::ListenAddress(address))
            }
            _ => None,
        }
    }

    /// Issue a random walk query to discover new peers.
    fn trigger_random_walk(&mut self) {
        let random_peer = PeerId::random();
        self.swarm
            .behaviour_mut()
            .kademlia
            .get_closest_peers(random_peer);
        debug!("kademlia random walk triggered");
    }

    /// Try to auto-dial a newly discovered peer (respects concurrency limit).
    fn try_discovery_dial(&mut self, peer_id: &PeerId, addrs: &[Multiaddr]) {
        if self.swarm.is_connected(peer_id) {
            return;
        }
        if *peer_id == *self.swarm.local_peer_id() {
            return;
        }
        if self.pending_discovery_dials >= self.max_discovery_dials {
            debug!(%peer_id, "skipping discovery dial (concurrency limit)");
            return;
        }

        let dial_result = if let Some(addr) = addrs.first() {
            debug!(%peer_id, %addr, "auto-dialing discovered peer");
            self.swarm.dial(addr.clone())
        } else {
            debug!(%peer_id, "auto-dialing discovered peer by peer_id");
            self.swarm.dial(*peer_id)
        };

        match dial_result {
            Ok(()) => {
                self.pending_discovery_dials += 1;
            }
            Err(e) => {
                debug!(%peer_id, error = %e, "failed to auto-dial discovered peer");
            }
        }
    }

    fn map_behaviour_event(&mut self, event: ZfsBehaviourEvent) -> Option<NetworkEvent> {
        match event {
            ZfsBehaviourEvent::Gossipsub(ev) => Self::map_gossip_event(ev),
            ZfsBehaviourEvent::RequestResponse(ev) => Self::map_reqresp_event(ev),
            ZfsBehaviourEvent::Kademlia(ev) => self.map_kademlia_event(ev),
        }
    }

    fn map_gossip_event(event: gossipsub::Event) -> Option<NetworkEvent> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message,
                ..
            } => Some(NetworkEvent::GossipMessage {
                source: message.source.or(Some(propagation_source)),
                topic: message.topic.to_string(),
                data: message.data,
            }),
            _ => None,
        }
    }

    fn map_reqresp_event(
        event: request_response::Event<ZfsRequest, ZfsResponse>,
    ) -> Option<NetworkEvent> {
        match event {
            request_response::Event::Message { peer, message, .. } => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => match request {
                    ZfsRequest::Store(req) => Some(NetworkEvent::IncomingStore {
                        peer,
                        request: req,
                        channel,
                    }),
                    ZfsRequest::Fetch(req) => Some(NetworkEvent::IncomingFetch {
                        peer,
                        request: req,
                        channel,
                    }),
                },
                request_response::Message::Response {
                    request_id,
                    response,
                } => match response {
                    ZfsResponse::Store(resp) => Some(NetworkEvent::StoreResult {
                        peer,
                        request_id,
                        response: resp,
                    }),
                    ZfsResponse::Fetch(resp) => Some(NetworkEvent::FetchResult {
                        peer,
                        request_id,
                        response: *resp,
                    }),
                },
            },
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
                ..
            } => Some(NetworkEvent::OutboundFailure {
                peer,
                request_id,
                error: error.to_string(),
            }),
            _ => None,
        }
    }

    fn map_kademlia_event(&mut self, event: kad::Event) -> Option<NetworkEvent> {
        match event {
            kad::Event::RoutingUpdated {
                peer, addresses, ..
            } => {
                let addrs: Vec<Multiaddr> = addresses.iter().cloned().collect();
                if self.discovered_peers.insert(peer) {
                    self.try_discovery_dial(&peer, &addrs);
                    Some(NetworkEvent::PeerDiscovered {
                        zode_id: peer,
                        addresses: addrs,
                    })
                } else {
                    None
                }
            }
            kad::Event::OutboundQueryProgressed {
                result: kad::QueryResult::GetClosestPeers(Ok(ok)),
                ..
            } => self.handle_closest_peers(ok),
            kad::Event::OutboundQueryProgressed {
                result: kad::QueryResult::Bootstrap(Ok(result)),
                ..
            } => {
                debug!(num_remaining = result.num_remaining, "kademlia bootstrap progressed");
                None
            }
            other => {
                debug!(?other, "kademlia event");
                None
            }
        }
    }

    fn handle_closest_peers(
        &mut self,
        ok: kad::GetClosestPeersOk,
    ) -> Option<NetworkEvent> {
        let mut first_new_peer = None;
        for peer in &ok.peers {
            let peer_id = peer.peer_id;
            if self.discovered_peers.insert(peer_id) {
                let addrs: Vec<Multiaddr> = peer.addrs.clone();
                self.try_discovery_dial(&peer_id, &addrs);
                if first_new_peer.is_none() {
                    first_new_peer = Some(NetworkEvent::PeerDiscovered {
                        zode_id: peer_id,
                        addresses: addrs,
                    });
                }
            }
        }
        first_new_peer
    }
}

fn build_swarm() -> Result<libp2p::Swarm<ZfsBehaviour>, NetworkError> {
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Permissive)
        .message_id_fn(message_id_fn)
        .build()
        .map_err(|e| NetworkError::Config(format!("{e}")))?;

    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetworkError::Transport(format!("{e}")))?
        .with_quic()
        .with_behaviour(|key| build_behaviour(key, gossipsub_config))
        .map_err(|e| NetworkError::Transport(format!("{e}")))?
        .build();
    Ok(swarm)
}

fn build_behaviour(
    key: &libp2p::identity::Keypair,
    gossipsub_config: gossipsub::Config,
) -> Result<ZfsBehaviour, Box<dyn std::error::Error + Send + Sync>> {
    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub_config,
    )?;
    let request_response = request_response::cbor::Behaviour::new(
        [(
            StreamProtocol::new(ZFS_PROTOCOL),
            request_response::ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    );
    let peer_id = key.public().to_peer_id();
    let mut kad_config = kad::Config::new(
        StreamProtocol::try_from_owned(ZFS_KAD_PROTOCOL.to_string())
            .expect("valid protocol name"),
    );
    kad_config.set_query_timeout(Duration::from_secs(60));
    let store = kad::store::MemoryStore::new(peer_id);
    let kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);
    Ok(ZfsBehaviour {
        gossipsub,
        request_response,
        kademlia,
    })
}

fn dial_bootstrap_peers(
    swarm: &mut libp2p::Swarm<ZfsBehaviour>,
    peers: &[Multiaddr],
    kademlia_enabled: bool,
) -> Result<(), NetworkError> {
    for peer_addr in peers {
        if kademlia_enabled {
            if let Some(peer_id) = extract_peer_id(peer_addr) {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, peer_addr.clone());
                debug!(%peer_id, %peer_addr, "added bootstrap peer to kademlia");
            }
        }
        swarm
            .dial(peer_addr.clone())
            .map_err(|e| NetworkError::Dial(e.to_string()))?;
    }
    Ok(())
}

fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|proto| match proto {
        libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}
