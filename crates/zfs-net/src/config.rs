use std::time::Duration;

use libp2p::Multiaddr;

/// Configuration for the ZFS network layer.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Address to listen on (e.g. `/ip4/0.0.0.0/udp/0/quic-v1`).
    pub listen_addr: Multiaddr,

    /// Bootstrap peers to connect to on startup.
    pub bootstrap_peers: Vec<Multiaddr>,

    /// Discovery settings (Kademlia DHT, mDNS).
    pub discovery: DiscoveryConfig,
}

/// Discovery configuration for Kademlia DHT and mDNS.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Enable Kademlia DHT for automatic peer discovery.
    /// Default: `true`.
    pub enable_kademlia: bool,

    /// Kademlia mode. Zodes should use `Server`; SDK clients should use `Client`.
    /// Default: `Server`.
    pub kademlia_mode: KademliaMode,

    /// Interval between random walk queries to discover new peers.
    /// Default: 30 seconds.
    pub random_walk_interval: Duration,

    /// Maximum number of concurrent outbound dials triggered by discovery.
    /// Default: 8.
    pub max_concurrent_discovery_dials: usize,
}

/// Kademlia operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KademliaMode {
    /// This Zode responds to DHT queries from other peers (long-lived Zodes).
    Server,
    /// This Zode queries the DHT but does not serve routing info (short-lived SDK clients).
    Client,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_kademlia: true,
            kademlia_mode: KademliaMode::Server,
            random_walk_interval: Duration::from_secs(30),
            max_concurrent_discovery_dials: 8,
        }
    }
}

impl NetworkConfig {
    pub fn new(listen_addr: Multiaddr) -> Self {
        Self {
            listen_addr,
            bootstrap_peers: Vec::new(),
            discovery: DiscoveryConfig::default(),
        }
    }

    pub fn with_bootstrap_peers(mut self, peers: Vec<Multiaddr>) -> Self {
        self.bootstrap_peers = peers;
        self
    }

    pub fn with_discovery(mut self, discovery: DiscoveryConfig) -> Self {
        self.discovery = discovery;
        self
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("well-known constant multiaddr"),
            bootstrap_peers: Vec::new(),
            discovery: DiscoveryConfig::default(),
        }
    }
}
