use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use contract_client::Client as ContractClient;
use subsquid_network_transport::PeerId;

use crate::allocations::AllocationsManager;
use crate::config::Config;
use crate::network_state::NetworkState;

pub struct ChainUpdatesHandler {
    network_state: Arc<RwLock<NetworkState>>,
    allocations_manager: Arc<RwLock<AllocationsManager>>,
    contract_client: Box<dyn ContractClient>,
    local_peer_id: PeerId,
}

impl ChainUpdatesHandler {
    pub fn new(
        network_state: Arc<RwLock<NetworkState>>,
        allocations_manager: Arc<RwLock<AllocationsManager>>,
        contract_client: Box<dyn ContractClient>,
        local_peer_id: PeerId,
    ) -> Self {
        Self {
            network_state,
            allocations_manager,
            contract_client,
            local_peer_id,
        }
    }

    pub async fn spawn(self) -> anyhow::Result<JoinHandle<()>> {
        self.handle_updates().await?;
        Ok(tokio::spawn(self.run()))
    }

    async fn run(self) {
        let workers_update_interval = Config::get().workers_update_interval;
        loop {
            tokio::time::sleep(workers_update_interval).await;
            let _ = self
                .handle_updates()
                .await
                .map_err(|e| log::error!("Error handling updates from chain: {e:?}"));
        }
    }

    async fn handle_updates(&self) -> anyhow::Result<()> {
        // Check if a new epoch has begun
        log::debug!("Checking for updates from chain");
        let alloc_manager = self.allocations_manager.read().await;
        let last_epoch = alloc_manager.get_last_epoch().await?;
        let current_epoch = self.contract_client.current_epoch().await?;
        if current_epoch == last_epoch {
            let (allocated, spent) = alloc_manager.compute_units_summary().await?;
            log::info!("allocated CU: {allocated} spent CU: {spent}");
            return Ok(());
        }

        log::info!("Epoch {current_epoch} has begun. Updating workers and allocations");
        // Get workers & allocations
        let workers = self.contract_client.active_workers().await?;
        let allocations = self
            .contract_client
            .current_allocations(self.local_peer_id, Some(workers.clone()))
            .await?;

        // Update workers & allocations
        alloc_manager
            .update_allocations(allocations, current_epoch)
            .await?;
        let mut network_state = self.network_state.write().await;
        network_state.update_registered_workers(workers);
        network_state.reset_allocations_cache();

        let (allocated, spent) = alloc_manager.compute_units_summary().await?;
        log::info!("Updating workers and allocations complete. allocated CU: {allocated} spent CU: {spent}");
        Ok(())
    }
}
