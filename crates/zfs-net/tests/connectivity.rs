use std::time::Duration;

use tokio::time::timeout;
use zfs_core::{Cid, FetchRequest, FetchResponse, ProgramId};
use zfs_net::{NetworkConfig, NetworkEvent, NetworkService};

async fn wait_for_listen_addr(zode: &mut NetworkService) -> zfs_net::Multiaddr {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::ListenAddress(addr)) = zode.next_event().await {
                return addr;
            }
        }
    })
    .await
    .expect("timed out waiting for listen address")
}

#[tokio::test]
async fn two_zode_connectivity() {
    let mut zode1 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let mut zode2 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let _zode1_addr = wait_for_listen_addr(&mut zode1).await;
    let zode2_addr = wait_for_listen_addr(&mut zode2).await;
    let zode2_zode_id = *zode2.local_zode_id();

    // Spawn zode2's event loop so it can accept the incoming connection.
    tokio::spawn(async move {
        loop {
            let _ = zode2.next_event().await;
        }
    });

    zode1.dial(zode2_addr).unwrap();

    let connected_peer = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::PeerConnected(peer)) = zode1.next_event().await {
                return peer;
            }
        }
    })
    .await
    .expect("connection timed out");

    assert_eq!(connected_peer, zode2_zode_id);
}

#[tokio::test]
async fn fetch_request_response_round_trip() {
    let mut zode1 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let mut zode2 = NetworkService::new(NetworkConfig::new(
        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
    ))
    .await
    .unwrap();

    let _zode1_addr = wait_for_listen_addr(&mut zode1).await;
    let zode2_addr = wait_for_listen_addr(&mut zode2).await;
    let zode2_zode_id = *zode2.local_zode_id();

    // Spawn zode2: handle incoming fetch requests with a fixed response.
    tokio::spawn(async move {
        loop {
            if let Some(NetworkEvent::IncomingFetch { channel, .. }) = zode2.next_event().await {
                let resp = FetchResponse {
                    ciphertext: Some(vec![0xAB; 64]),
                    head: None,
                    error_code: None,
                };
                zode2
                    .send_fetch_response(channel, resp)
                    .expect("send response");
            }
        }
    });

    zode1.dial(zode2_addr).unwrap();

    // Wait for connection before sending request.
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::PeerConnected(_)) = zode1.next_event().await {
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
    zode1.send_fetch(&zode2_zode_id, request);

    let response = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::FetchResult { response, .. }) = zode1.next_event().await {
                return response;
            }
        }
    })
    .await
    .expect("fetch response timed out");

    assert_eq!(response.ciphertext, Some(vec![0xAB; 64]));
    assert!(response.error_code.is_none());
}
