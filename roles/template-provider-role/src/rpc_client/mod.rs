use crate::{common_capnp, init_capnp, mining_capnp, proxy_capnp};
use capnp::capability::Promise;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use error::RpcClientError;
use futures::FutureExt;
use std::{io::Cursor, path::Path, sync::Arc};
use stratum_common::bitcoin::{consensus::Decodable, hashes::Hash, Block, BlockHash};

use tokio::{net::UnixStream, task};
use tokio_util::compat::*;
use tracing::{debug, error, info};

pub mod error;

pub struct BackendRpcClient {
    bootstrap_client: init_capnp::init::Client,
    rpc_execution_thread: Arc<proxy_capnp::thread::Client>,
}

impl BackendRpcClient {
    pub async fn connect(socket_path: &Path) -> Result<Self, RpcClientError> {
        info!("Connection RPC client to socket: {}", socket_path.display());
        let stream = UnixStream::connect(socket_path).await?;
        let (reader, writer) = stream.into_split();
        let reader = reader.compat();
        let writer = writer.compat_write();

        let network = Box::new(twoparty::VatNetwork::new(
            reader,
            writer,
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        ));

        let mut rpc_system = RpcSystem::new(network, None);
        let bootstrap_client: init_capnp::init::Client =
            rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

        task::spawn_local(rpc_system.map(|_| debug!("RPC system finished")));
        info!("RPC System spawned locally.");

        let construct_request = bootstrap_client.construct_request();
        let construct_response = construct_request.send().promise.await?;

        let thread_map = construct_response.get()?.get_thread_map()?;
        debug!("Construct response received. Requesting execution thread handle...");

        let thread_response = thread_map.make_thread_request().send().promise.await?;

        let rpc_execution_thread = thread_response.get()?.get_result()?;

        info!("RPC execution thread handle obtained successfully.");

        Ok(Self {
            bootstrap_client,
            rpc_execution_thread: Arc::new(rpc_execution_thread),
        })
    }

    async fn get_mining_service(&self) -> Result<mining_capnp::mining::Client, RpcClientError> {
        debug!("Requesting Mining service capability...");
        let mut request = self.bootstrap_client.make_mining_request();
        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());
        let response = request.send().promise.await?;
        response.get()?.get_result().map_err(RpcClientError::Capnp)
    }

    pub async fn get_tip(&self) -> Result<Option<(BlockHash, u32)>, RpcClientError> {
        let mining = self.get_mining_service().await?;
        let mut request = mining.get_tip_request();
        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());

        let response = request.send().promise.await?;

        if !response.get()?.get_has_result() {
            debug!("Node reported no current tip available");
            return Ok(None);
        }

        let result_reader = response.get()?.get_result()?;

        let hash_bytes = result_reader.get_hash()?;
        let height = result_reader.get_height();

        let block_hash = BlockHash::from_slice(hash_bytes)
            .map_err(|e| RpcClientError::InvalidData(format!("Invalid tip hash bytes: {}", e)))?;

        debug!("Tip received: height={}, hash={}", height, block_hash);
        Ok(Some((block_hash, height as u32)))
    }

    pub async fn update_coinbase_constraints(
        &self,
        max_additional_sigops: u64,
        reserved_weight: u64,
    ) -> Result<(), RpcClientError> {
        info!(
            max_additional_sigops,
            reserved_weight, "Updating coinbase constraints via RPC options."
        );

        let mining = self.get_mining_service().await?;
        let mut create_block_req = mining.create_new_block_request();

        let mut options = create_block_req.get().init_options();
        options.set_block_reserved_weight(reserved_weight);
        options.set_coinbase_output_max_additional_sigops(max_additional_sigops);
        options.set_use_mempool(true);

        let _ = create_block_req.send().promise.await?;
        Ok(())
    }

    pub async fn create_new_block(
        &self,
        use_mempool: bool,
        reserved_weight: u64,
        max_additional_sigops: u64,
    ) -> Result<BlockTemplateClient, RpcClientError> {
        let mining = self.get_mining_service().await?;
        let mut request = mining.create_new_block_request();

        let mut options = request.get().init_options();
        options.set_use_mempool(use_mempool);
        options.set_block_reserved_weight(reserved_weight);
        options.set_coinbase_output_max_additional_sigops(max_additional_sigops);
        let response = request.send().promise.await?;
        let template_client_capnp = response.get()?.get_result()?;
        debug!("Received BlockTemplate capability.");

        Ok(BlockTemplateClient::new(
            template_client_capnp,
            self.rpc_execution_thread.clone(),
        ))
    }
}

pub struct BlockTemplateClient {
    template_capnp: mining_capnp::block_template::Client,
    rpc_execution_thread: Arc<proxy_capnp::thread::Client>,
}

impl BlockTemplateClient {
    fn new(
        template_capnp: mining_capnp::block_template::Client,
        rpc_execution_thread: Arc<proxy_capnp::thread::Client>,
    ) -> Self {
        Self {
            template_capnp,
            rpc_execution_thread,
        }
    }

    pub async fn get_block(&self) -> Result<Block, RpcClientError> {
        debug!("Requesting block data for template");
        let mut request = self.template_capnp.get_block_request();
        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());
        let response = request.send().promise.await?;
        let block_bytes = response.get()?.get_result()?;
        debug!("Received raw block data ({} bytes).", block_bytes.len());
        let mut cursor = Cursor::new(block_bytes);
        Block::consensus_decode(&mut cursor).map_err(RpcClientError::Encode)
    }

    pub async fn get_coinbase_merkle_path(&self) -> Result<Vec<Vec<u8>>, RpcClientError> {
        debug!("Requesting coinbase merkle path for template...");
        let mut request = self.template_capnp.get_coinbase_merkle_path_request();
        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());
        let response = request.send().promise.await?;
        let list_reader = response.get()?.get_result()?;
        debug!("Received merkle path ({} elements).", list_reader.len());
        list_reader
            .iter()
            .map(|result| result.map(|bytes| bytes.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(RpcClientError::Capnp)
    }

    pub async fn submit_solution(
        &self,
        version: u32,
        timestamp: u32,
        nonce: u32,
        coinbase_txn_bytes: &[u8],
    ) -> Result<bool, RpcClientError> {
        let mut request = self.template_capnp.submit_solution_request();
        request.get().set_version(version);
        request.get().set_timestamp(timestamp);
        request.get().set_nonce(nonce);
        request.get().set_coinbase(coinbase_txn_bytes);

        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());

        let response = request.send().promise.await?;
        let success = response.get()?.get_result();
        info!("Solution submission result: {}", success);
        if success {
            Ok(true)
        } else {
            Err(RpcClientError::OperationFailed(
                "SubmitSolution returned false".to_string(),
            ))
        }
    }

    pub async fn destroy(self) -> Result<(), RpcClientError> {
        debug!("Destroying BlockTemplate capability...");
        let mut request = self.template_capnp.destroy_request();
        request
            .get()
            .get_context()?
            .set_thread(self.rpc_execution_thread.as_ref().clone());

        request.send().promise.await?;
        debug!("BlockTemplate destroy request sent.");
        Ok(())
    }
}
