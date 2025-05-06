use std::{collections::HashMap, time::Instant};

use stratum_common::bitcoin::{hashes::Hash, Block, BlockHash};
use tracing::debug;

const DEFAULT_MAX_ADDITIONAL_SIGOPS: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTip {
    pub prevhash: BlockHash,
    pub height: i32,
}

impl Default for ChainTip {
    fn default() -> Self {
        ChainTip {
            prevhash: BlockHash::from_byte_array([0; 32]),
            height: -1,
        }
    }
}

#[derive(Debug)]
pub struct CoolDownTransactionData {
    pub block: Block,
    pub timeout: Instant,
}

#[derive(Debug)]
pub struct ProviderState {
    template_id_counter: u64,
    pub last_tip: ChainTip,
    is_prev_hash_received: bool,
    pub templates: HashMap<u64, CoolDownTransactionData>,
    pub current_fee: u64,
    is_coinbase_constraint_received: bool,
    coinbase_max_additional_sigops: Option<u64>,
}

impl ProviderState {
    pub fn new() -> Self {
        ProviderState {
            template_id_counter: 0,
            last_tip: ChainTip::default(),
            is_prev_hash_received: false,
            templates: HashMap::new(),
            current_fee: 0,
            is_coinbase_constraint_received: false,
            coinbase_max_additional_sigops: None,
        }
    }

    pub fn get_next_template_id(&mut self) -> u64 {
        let id = self.template_id_counter;
        self.template_id_counter += 1;
        id
    }

    pub fn peek_next_template_id(&self) -> u64 {
        self.template_id_counter + 1
    }

    pub fn set_prev_hash_received(&mut self, received: bool) {
        self.is_prev_hash_received = received;
    }

    pub fn is_prev_hash_received(&self) -> bool {
        self.is_coinbase_constraint_received
    }

    pub fn set_coinbase_constraints(&mut self, max_additional_sigops: u64) {
        self.coinbase_max_additional_sigops = Some(max_additional_sigops);
        self.is_coinbase_constraint_received = true;
    }

    pub fn is_coinbase_constraint_received(&self) -> bool {
        self.is_coinbase_constraint_received
    }

    pub fn get_coinbase_max_additional_sigops(&self) -> u64 {
        self.coinbase_max_additional_sigops
            .unwrap_or(DEFAULT_MAX_ADDITIONAL_SIGOPS)
    }

    pub fn insert_template(&mut self, template_id: u64, block: Block) {
        debug!("Storing block data for template ID {}", template_id);
        self.templates.insert(
            template_id,
            CoolDownTransactionData {
                block,
                timeout: Instant::now(),
            },
        );
    }
}
