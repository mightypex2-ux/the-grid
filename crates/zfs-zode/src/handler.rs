use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, warn};
use zfs_core::{
    Cid, FetchRequest, FetchResponse, GossipBlock, ProgramId, StoreRequest, StoreResponse,
};
use zfs_proof::ProofVerifier;
use zfs_storage::StorageBackend;

use crate::config::{LimitsConfig, ProofPolicyConfig};
use crate::error::ZodeError;
use crate::metrics::ZodeMetrics;

/// Handles incoming store and fetch requests, enforcing policy, proof
/// verification, and size limits before persisting via `zfs-storage`.
pub(crate) struct RequestHandler<S> {
    storage: Arc<S>,
    topics: HashSet<ProgramId>,
    limits: LimitsConfig,
    proof_policy: ProofPolicyConfig,
    verifier: Arc<dyn ProofVerifier>,
    metrics: Arc<ZodeMetrics>,
}

impl<S: StorageBackend> RequestHandler<S> {
    pub(crate) fn new(
        storage: Arc<S>,
        topics: HashSet<ProgramId>,
        limits: LimitsConfig,
        proof_policy: ProofPolicyConfig,
        verifier: Arc<dyn ProofVerifier>,
        metrics: Arc<ZodeMetrics>,
    ) -> Self {
        Self {
            storage,
            topics,
            limits,
            proof_policy,
            verifier,
            metrics,
        }
    }

    /// Process a store request. Returns the response to send back.
    pub(crate) fn handle_store(&self, req: &StoreRequest) -> StoreResponse {
        match self.handle_store_inner(req) {
            Ok(()) => {
                self.metrics.inc_blocks_stored();
                StoreResponse {
                    ok: true,
                    error_code: None,
                }
            }
            Err(e) => {
                let code = e.to_error_code();
                debug!(error = %e, ?code, "store request rejected");
                StoreResponse {
                    ok: false,
                    error_code: Some(code),
                }
            }
        }
    }

    fn handle_store_inner(&self, req: &StoreRequest) -> Result<(), ZodeError> {
        self.check_store_policy(req)?;
        self.check_block_size(req.ciphertext.len())?;
        self.verify_store_proof(req)?;
        self.check_db_capacity(req.ciphertext.len())?;
        self.persist_block(&req.cid, &req.ciphertext, req.head.as_ref(), &req.program_id)
    }

    fn check_store_policy(&self, req: &StoreRequest) -> Result<(), ZodeError> {
        if !self.topics.contains(&req.program_id) {
            self.metrics.inc_policy_rejection();
            return Err(ZodeError::PolicyReject(format!(
                "program {} not in subscribed topics",
                req.program_id.to_hex()
            )));
        }
        Ok(())
    }

    fn check_block_size(&self, size: usize) -> Result<(), ZodeError> {
        if let Some(max_block) = self.limits.max_block_size_bytes {
            if size as u64 > max_block {
                self.metrics.inc_limit_rejection();
                return Err(ZodeError::StorageFull(format!(
                    "block size {} exceeds limit {}",
                    size, max_block
                )));
            }
        }
        Ok(())
    }

    fn verify_store_proof(&self, req: &StoreRequest) -> Result<(), ZodeError> {
        if !self.proof_required(&req.program_id) {
            return Ok(());
        }
        match &req.proof {
            Some(proof) => {
                let version = req.head.as_ref().map_or(0, |h| h.version);
                self.verifier
                    .verify(&req.cid, &req.program_id, version, proof, None)
                    .map(|_| ())
                    .map_err(|e| {
                        self.metrics.inc_proof_rejection();
                        ZodeError::Proof(e)
                    })
            }
            None => {
                self.metrics.inc_proof_rejection();
                Err(ZodeError::PolicyReject(
                    "proof required but not provided".into(),
                ))
            }
        }
    }

    fn check_db_capacity(&self, additional_bytes: usize) -> Result<(), ZodeError> {
        if let Some(max_total) = self.limits.max_total_db_bytes {
            let stats = self.storage.stats().map_err(ZodeError::Storage)?;
            if stats.db_size_bytes + additional_bytes as u64 > max_total {
                self.metrics.inc_limit_rejection();
                return Err(ZodeError::StorageFull(
                    "total DB size limit exceeded".into(),
                ));
            }
        }
        Ok(())
    }

    fn persist_block(
        &self,
        cid: &Cid,
        ciphertext: &[u8],
        head: Option<&zfs_core::Head>,
        program_id: &ProgramId,
    ) -> Result<(), ZodeError> {
        self.storage
            .put(cid, ciphertext)
            .map_err(ZodeError::Storage)?;
        if let Some(head) = head {
            self.storage
                .put_head(&head.sector_id, head)
                .map_err(ZodeError::Storage)?;
        }
        self.storage
            .add_cid(program_id, cid)
            .map_err(ZodeError::Storage)?;
        if let Ok(stats) = self.storage.stats() {
            self.metrics.set_db_size(stats.db_size_bytes);
        }
        Ok(())
    }

    /// Process a block received via GossipSub.  Returns `true` if stored.
    pub(crate) fn handle_gossip(&self, block: &GossipBlock) -> bool {
        if !self.topics.contains(&block.program_id) {
            self.metrics.inc_policy_rejection();
            return false;
        }

        let expected_cid = Cid::from_ciphertext(&block.ciphertext);
        if block.cid != expected_cid {
            debug!("gossip block CID mismatch, dropping");
            return false;
        }

        if let Some(max_block) = self.limits.max_block_size_bytes {
            if block.ciphertext.len() as u64 > max_block {
                self.metrics.inc_limit_rejection();
                return false;
            }
        }

        if let Some(max_total) = self.limits.max_total_db_bytes {
            if let Ok(stats) = self.storage.stats() {
                if stats.db_size_bytes + block.ciphertext.len() as u64 > max_total {
                    self.metrics.inc_limit_rejection();
                    return false;
                }
            }
        }

        if self.storage.put(&block.cid, &block.ciphertext).is_err() {
            return false;
        }
        if let Some(ref head) = block.head {
            let _ = self.storage.put_head(&head.sector_id, head);
        }
        let _ = self.storage.add_cid(&block.program_id, &block.cid);

        if let Ok(stats) = self.storage.stats() {
            self.metrics.set_db_size(stats.db_size_bytes);
        }
        self.metrics.inc_blocks_stored();
        true
    }

    /// Process a fetch request. Returns the response to send back.
    pub(crate) fn handle_fetch(&self, req: &FetchRequest) -> FetchResponse {
        match self.handle_fetch_inner(req) {
            Ok(resp) => resp,
            Err(e) => {
                let code = e.to_error_code();
                warn!(error = %e, ?code, "fetch request failed");
                FetchResponse {
                    ciphertext: None,
                    head: None,
                    error_code: Some(code),
                }
            }
        }
    }

    fn handle_fetch_inner(&self, req: &FetchRequest) -> Result<FetchResponse, ZodeError> {
        // 1. Check program allowlist
        if !self.topics.contains(&req.program_id) {
            return Err(ZodeError::PolicyReject(format!(
                "program {} not in subscribed topics",
                req.program_id.to_hex()
            )));
        }

        // 2. Lookup by CID
        if let Some(ref cid) = req.by_cid {
            let ciphertext = self.storage.get(cid).map_err(ZodeError::Storage)?;
            return match ciphertext {
                Some(ct) => Ok(FetchResponse {
                    ciphertext: Some(ct),
                    head: None,
                    error_code: None,
                }),
                None => Err(ZodeError::Core(zfs_core::ZfsError::NotFound)),
            };
        }

        // 3. Lookup by sector_id (return head)
        if let Some(ref sector_id) = req.by_sector_id {
            let head = self
                .storage
                .get_head(sector_id)
                .map_err(ZodeError::Storage)?;
            return match head {
                Some(h) => {
                    let ciphertext = self.storage.get(&h.cid).map_err(ZodeError::Storage)?;
                    Ok(FetchResponse {
                        ciphertext,
                        head: Some(h),
                        error_code: None,
                    })
                }
                None => Err(ZodeError::Core(zfs_core::ZfsError::NotFound)),
            };
        }

        Err(ZodeError::Core(zfs_core::ZfsError::InvalidPayload(
            "fetch request must specify by_cid or by_sector_id".into(),
        )))
    }

    fn proof_required(&self, program_id: &ProgramId) -> bool {
        if self.proof_policy.require_proofs {
            return true;
        }
        self.proof_policy
            .programs_requiring_proof
            .contains(program_id)
    }

    /// Verify a CID matches the ciphertext (integrity check).
    #[allow(dead_code)]
    pub(crate) fn verify_cid(cid: &Cid, ciphertext: &[u8]) -> bool {
        let expected = Cid::from_ciphertext(ciphertext);
        cid == &expected
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
