use crate::gas::AggregatedGasPriceReader;
use crate::rest::api_server_state::{ApiServerState, FaucetState};
use crate::rest::faucet::Faucet;
use crate::rest::faucet_call_limiter::FaucetCallLimiter;
use crate::rest::rest_call_limiter::RestCallLimiter;
use crate::transactions::dispatcher::TransactionDispatcher;
use crate::RpcStorage;
use execution::MoveStore;
use mempool::MoveSequenceNumberProvider;
use rocksstore::chain_storage::block_store::ChainStore;
use std::sync::{Arc, RwLock};
use types::limiting_strategy::LimitingStrategy;

#[derive(Default)]
pub struct ApiServerStateBuilder {
    storage: Option<RpcStorage<MoveSequenceNumberProvider>>,
    move_store: Option<MoveStore>,
    store: Option<ChainStore>,
    tx_sender: Option<TransactionDispatcher>,
    gas_price_reader: Option<AggregatedGasPriceReader>,
    rest_call_limiter: Option<RestCallLimiter>,
    faucet: Option<(Faucet, FaucetCallLimiter)>,
}

impl ApiServerStateBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn storage(mut self, storage: RpcStorage<MoveSequenceNumberProvider>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn move_store(mut self, move_store: MoveStore) -> Self {
        self.move_store = Some(move_store);
        self
    }

    pub fn store(mut self, store: ChainStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn tx_sender(mut self, tx_sender: TransactionDispatcher) -> Self {
        self.tx_sender = Some(tx_sender);
        self
    }

    pub fn gas_price_reader(mut self, gas_price_reader: AggregatedGasPriceReader) -> Self {
        self.gas_price_reader = Some(gas_price_reader);
        self
    }

    /// Sets limiting strategy for tx submission API calls.
    pub fn rest_call_limiter(mut self, strategy: LimitingStrategy) -> Self {
        self.rest_call_limiter = Some(RestCallLimiter::new(strategy));
        self
    }

    pub fn faucet(mut self, faucet: Faucet, faucet_call_limiter: FaucetCallLimiter) -> Self {
        self.faucet = Some((faucet, faucet_call_limiter));
        self
    }

    pub fn build(self) -> ApiServerState {
        let move_store = self.move_store.expect("Move store must be set");
        let chain_id = move_store
            .chain_id()
            .expect("Chain ID must be available")
            .chain_id()
            .id();
        ApiServerState {
            storage: self.storage.expect("RPC Storage must be set"),
            move_store,
            store: self.store.expect("Store must be set"),
            tx_sender: self.tx_sender.expect("Tx Sender must be set"),
            gas_price_reader: self.gas_price_reader.expect("Gas Price Reader must be set"),
            rest_call_limiter: self
                .rest_call_limiter
                .map(|limiter| Arc::new(RwLock::new(limiter))),
            faucet_state: self
                .faucet
                .map(|(faucet, limiter)| FaucetState::new(faucet, limiter)),
            chain_id,
        }
    }
}
