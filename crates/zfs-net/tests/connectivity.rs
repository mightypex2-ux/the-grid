use std::time::Duration;

use tokio::time::timeout;
use zfs_core::{Cid, FetchRequest, FetchResponse, ProgramId};
use zfs_net::{NetworkConfig, NetworkEvent, NetworkService};

async fn wait_for_listen_addr(node: &mut NetworkService) -> zfs_net::Multiaddr {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::ListenAddress(addr)) = node.next_event().await {
                return addr;
            }
        }
    })
    .await
    .expect("timed out waiting for listen address")
}

#[tokio::test]
async fn two_node_connectivity() {
    let mut node1 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let mut node2 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let _node1_addr = wait_for_listen_addr(&mut node1).await;
    let node2_addr = wait_for_listen_addr(&mut node2).await;
    let node2_peer_id = *node2.local_peer_id();

    // Spawn node2's event loop so it can accept the incoming connection.
    tokio::spawn(async move {
        loop {
            let _ = node2.next_event().await;
        }
    });

    node1.dial(node2_addr).unwrap();

    let connected_peer = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::PeerConnected(peer)) = node1.next_event().await {
                return peer;
            }
        }
    })
    .await
    .expect("connection timed out");

    assert_eq!(connected_peer, node2_peer_id);
}

#[tokio::test]
async fn fetch_request_response_round_trip() {
    let mut node1 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let mut node2 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let _node1_addr = wait_for_listen_addr(&mut node1).await;
    let node2_addr = wait_for_listen_addr(&mut node2).await;
    let node2_peer_id = *node2.local_peer_id();

    // Spawn node2: handle incoming fetch requests with a fixed response.
    tokio::spawn(async move {
        loop {
            if let Some(NetworkEvent::IncomingFetch { channel, .. }) = node2.next_event().await {
                let resp = FetchResponse {
                    ciphertext: Some(vec![0xAB; 64]),
                    head: None,
                    error_code: None,
                };
                node2
                    .send_fetch_response(channel, resp)
                    .expect("send response");
            }
        }
    });

    node1.dial(node2_addr).unwrap();

    // Wait for connection before sending request.
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::PeerConnected(_)) = node1.next_event().await {
                return;
            }
        }
    })
    .await
    .expect("connection timed out");

    let request = FetchRequest {
        program_id: ProgramId::from([0u8; 32]),
        by_cid: Some(Cid::from([1u8; 32])),
        by_sector_id: None,
        machine_did: None,
        signature: None,
    };
    node1.send_fetch(&node2_peer_id, request);

    let response = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::FetchResult { response, .. }) = node1.next_event().await {
                return response;
            }
        }
    })
    .await
    .expect("fetch response timed out");

    assert_eq!(response.ciphertext, Some(vec![0xAB; 64]));
    assert!(response.error_code.is_none());
}
