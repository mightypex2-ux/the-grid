use sha2::{Digest, Sha256};
use zero_neural::{ed25519_to_did_key, MachineKeyPair};
use zfs_core::{
    Cid, ErrorCode, FetchRequest, FetchResponse, Head, ProgramId, SectorId, StoreRequest,
};
use zfs_net::PeerId;

use crate::client::{Client, PendingRequest};
use crate::error::SdkError;

/// Result of a successful upload to one or more Zodes.
#[derive(Debug, Clone)]
pub struct StoreResult {
    /// CID of the stored block.
    pub cid: Cid,
    /// Number of Zodes that successfully stored the block.
    pub successes: usize,
    /// Number of Zodes targeted (replication factor).
    pub replication_factor: usize,
    /// Per-peer results (peer_id, ok, error_code).
    pub peer_results: Vec<(String, bool, Option<ErrorCode>)>,
}

/// Result of a successful fetch.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// The fetched ciphertext.
    pub ciphertext: Vec<u8>,
    /// The sector head, if returned.
    pub head: Option<Head>,
}

/// Upload ciphertext to R Zodes, signed by the machine key.
///
/// Sends a `StoreRequest` to up to `replication_factor` connected peers.
/// Returns success if **at least one** peer stores the block.
pub async fn upload(
    client: &Client,
    machine_key: &MachineKeyPair,
    program_id: &ProgramId,
    ciphertext: &[u8],
    head: Option<&Head>,
    proof: Option<&[u8]>,
    replication_factor: usize,
) -> Result<StoreResult, SdkError> {
    let cid = Cid::from_ciphertext(ciphertext);
    let machine_did = ed25519_to_did_key(&machine_key.public_key().ed25519_bytes());

    let sign_payload = build_store_sign_payload(program_id, &cid, ciphertext);
    let signature = machine_key.sign(&sign_payload);

    let request = StoreRequest {
        program_id: *program_id,
        cid,
        ciphertext: ciphertext.to_vec(),
        head: head.cloned(),
        proof: proof.map(|p| p.to_vec()),
        key_envelope: None,
        machine_did,
        signature,
    };

    let peers = client.connected_peers().await;
    if peers.is_empty() {
        return Err(SdkError::NoPeers);
    }

    let target_count = replication_factor.min(peers.len());
    let targets: Vec<PeerId> = peers.into_iter().take(target_count).collect();

    let mut receivers = Vec::with_capacity(target_count);

    for peer in &targets {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request_id = {
            let mut net = client.network.lock().await;
            net.send_store(peer, request.clone())
        };
        {
            let mut pending = client.pending.lock().await;
            pending.insert(request_id, PendingRequest::Store(tx));
        }
        receivers.push((*peer, rx));
    }

    let mut peer_results = Vec::with_capacity(target_count);
    let mut successes = 0usize;

    for (peer, rx) in receivers {
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => {
                if resp.ok {
                    successes += 1;
                }
                peer_results.push((peer.to_string(), resp.ok, resp.error_code));
            }
            Ok(Err(_)) => {
                peer_results.push((peer.to_string(), false, None));
            }
            Err(_) => {
                peer_results.push((peer.to_string(), false, None));
            }
        }
    }

    if successes == 0 {
        return Err(SdkError::InsufficientReplication {
            successes: 0,
            required: replication_factor,
        });
    }

    Ok(StoreResult {
        cid,
        successes,
        replication_factor,
        peer_results,
    })
}

/// Fetch ciphertext by CID from a connected Zode.
pub async fn fetch(
    client: &Client,
    program_id: &ProgramId,
    cid: &Cid,
) -> Result<FetchResult, SdkError> {
    let request = FetchRequest {
        program_id: *program_id,
        by_cid: Some(*cid),
        by_sector_id: None,
        machine_did: None,
        signature: None,
    };

    let response = send_fetch(client, &request).await?;
    response_to_fetch_result(response)
}

/// Fetch head (and optionally ciphertext) by sector ID.
pub async fn fetch_head(
    client: &Client,
    program_id: &ProgramId,
    sector_id: &SectorId,
) -> Result<Option<Head>, SdkError> {
    let request = FetchRequest {
        program_id: *program_id,
        by_cid: None,
        by_sector_id: Some(sector_id.clone()),
        machine_did: None,
        signature: None,
    };

    let response = send_fetch(client, &request).await?;
    match response.error_code {
        Some(ErrorCode::NotFound) => Ok(None),
        Some(code) => Err(SdkError::Core(zfs_core::ZfsError::from(code))),
        None => Ok(response.head),
    }
}

async fn send_fetch(client: &Client, request: &FetchRequest) -> Result<FetchResponse, SdkError> {
    let peers = client.connected_peers().await;
    if peers.is_empty() {
        return Err(SdkError::NoPeers);
    }

    let peer = peers[0];
    let (tx, rx) = tokio::sync::oneshot::channel();
    let request_id = {
        let mut net = client.network.lock().await;
        net.send_fetch(&peer, request.clone())
    };
    {
        let mut pending = client.pending.lock().await;
        pending.insert(request_id, PendingRequest::Fetch(tx));
    }

    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(_)) => Err(SdkError::Other("fetch response channel dropped".into())),
        Err(_) => Err(SdkError::Timeout("fetch timed out after 30s".into())),
    }
}

fn response_to_fetch_result(response: FetchResponse) -> Result<FetchResult, SdkError> {
    if let Some(code) = response.error_code {
        return Err(SdkError::Core(zfs_core::ZfsError::from(code)));
    }
    match response.ciphertext {
        Some(ct) => Ok(FetchResult {
            ciphertext: ct,
            head: response.head,
        }),
        None => Err(SdkError::NotFound),
    }
}

fn build_store_sign_payload(program_id: &ProgramId, cid: &Cid, ciphertext: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"zfs:store:v1:");
    hasher.update(program_id.as_bytes());
    hasher.update(cid.as_bytes());
    hasher.update(ciphertext);
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_sign_payload_is_deterministic() {
        let pid = ProgramId::from_descriptor_bytes(b"test");
        let cid = Cid::from_ciphertext(b"hello");
        let p1 = build_store_sign_payload(&pid, &cid, b"hello");
        let p2 = build_store_sign_payload(&pid, &cid, b"hello");
        assert_eq!(p1, p2);
    }

    #[test]
    fn store_sign_payload_differs_with_different_data() {
        let pid = ProgramId::from_descriptor_bytes(b"test");
        let cid1 = Cid::from_ciphertext(b"hello");
        let cid2 = Cid::from_ciphertext(b"world");
        let p1 = build_store_sign_payload(&pid, &cid1, b"hello");
        let p2 = build_store_sign_payload(&pid, &cid2, b"world");
        assert_ne!(p1, p2);
    }
}
