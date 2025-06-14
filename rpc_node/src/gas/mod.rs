use crate::gas::online_mean::OnlineMean;
use execution::traits::TGasPriceUpdater;
use std::collections::HashMap;
use tokio::sync::watch;
use tracing::warn;
use transactions::TransactionKind;
use types::api;

mod circular_buf;
mod online_mean;

#[derive(Copy, Clone, Debug)]
/// A collection of statistics on gas prices.
pub struct GasPriceStatistics {
    mean: u64,
    max: u64,
    median: u64,
}

impl GasPriceStatistics {
    /// Minimum gas unit price for Move VM.
    pub const MVM_MIN_GAS_UNIT_PRICE: u64 = 100;

    /// Minimum number of gas units Move VM may use.
    pub const MVM_MIN_AMOUNT_GAS_UNIT: u64 = 2;

    pub fn mvm_new() -> Self {
        Self {
            mean: Self::MVM_MIN_GAS_UNIT_PRICE,
            max: Self::MVM_MIN_GAS_UNIT_PRICE,
            median: Self::MVM_MIN_GAS_UNIT_PRICE,
        }
    }
}
impl From<&GasPriceStatistics> for api::v1::GasPriceRes {
    fn from(value: &GasPriceStatistics) -> Self {
        Self {
            mean_gas_price: value.mean,
            max_gas_price: value.max,
        }
    }
}

impl From<GasPriceStatistics> for api::v1::GasPriceRes {
    fn from(value: GasPriceStatistics) -> Self {
        api::v1::GasPriceRes::from(&value)
    }
}

impl From<&GasPriceStatistics> for api::v2::GasPriceResV2 {
    fn from(value: &GasPriceStatistics) -> Self {
        Self {
            mean_gas_price: value.mean,
            max_gas_price: value.max,
            median_gas_price: value.median,
        }
    }
}

impl From<GasPriceStatistics> for api::v2::GasPriceResV2 {
    fn from(value: GasPriceStatistics) -> Self {
        api::v2::GasPriceResV2::from(&value)
    }
}

/// A generic watcher maintaining the recent gas prices updates **independent** of a particular VM.
struct GasPriceWriter {
    mean: OnlineMean,
    gas_tx: watch::Sender<GasPriceStatistics>,
}

/// An aggregated watcher maintaining the recent gas prices updates
/// -- [GasPriceStatistics] -- across different VMs.
#[derive(Default)]
pub struct AggregatedGasPriceWriter {
    gas_stats_per_tx_kind: HashMap<TransactionKind, GasPriceWriter>,
}

impl AggregatedGasPriceWriter {
    fn push_gas_price(&mut self, tx_kind: TransactionKind, gas_prices: Vec<u64>) {
        let Some(writer) = self.gas_stats_per_tx_kind.get_mut(&tx_kind) else {
            warn!("No gas prices stats are maintained for {tx_kind:?}");
            return;
        };

        writer.mean.push(gas_prices);

        let mean = writer.mean.mean();
        let max = writer.mean.max();
        let median = writer.mean.median();
        writer
            .gas_tx
            .send_replace(GasPriceStatistics { mean, max, median });
    }
}

impl TGasPriceUpdater for AggregatedGasPriceWriter {
    fn push_gas_prices(&mut self, tx_kind: TransactionKind, gas_prices: Vec<u64>) {
        self.push_gas_price(tx_kind, gas_prices);
    }
}

#[derive(Clone, Default)]
/// A structure monitoring the recent gas prices.
pub struct AggregatedGasPriceReader {
    /// A set of watchers for the last updates to gas usage by transaction kind.
    gas_stats_per_tx_kind: HashMap<TransactionKind, watch::Receiver<GasPriceStatistics>>,
}

impl AggregatedGasPriceReader {
    /// Mean gas usage by Move VM over a set of transactions.
    pub fn mean(&self, tx_kind: TransactionKind) -> Option<u64> {
        let mean = self
            .gas_stats_per_tx_kind
            .get(&tx_kind)
            .map(|reader| reader.borrow().mean);

        if mean.is_none() {
            warn!("No gas prices stats are not maintained for {tx_kind:?}");
        };

        mean
    }

    /// Max gas consumption by a transaction in Move VM.
    pub fn max(&self, tx_kind: TransactionKind) -> Option<u64> {
        let max = self
            .gas_stats_per_tx_kind
            .get(&tx_kind)
            .map(|reader| reader.borrow().max);

        if max.is_none() {
            warn!("No gas prices stats are not maintained for {tx_kind:?}");
        };

        max
    }

    /// Median gas usage by Move VM over a set of transactions.
    pub fn median(&self, tx_kind: TransactionKind) -> Option<u64> {
        let median = self
            .gas_stats_per_tx_kind
            .get(&tx_kind)
            .map(|reader| reader.borrow().median);

        if median.is_none() {
            warn!("Gas prices stats are not maintained for {tx_kind:?}");
        };

        median
    }

    pub fn statistics(&self, tx_kind: TransactionKind) -> Option<GasPriceStatistics> {
        let Some(reader) = self.gas_stats_per_tx_kind.get(&tx_kind) else {
            warn!("Gas prices stats are maintained for {tx_kind:?}");
            return None;
        };

        Some(GasPriceStatistics {
            mean: reader.borrow().mean,
            max: reader.borrow().max,
            median: reader.borrow().median,
        })
    }
}

pub fn gas_monitor() -> (AggregatedGasPriceWriter, AggregatedGasPriceReader) {
    let mut writer = AggregatedGasPriceWriter::default();
    let mut reader = AggregatedGasPriceReader::default();

    {
        // Initialize data for MoveVM and insert in the aggregated reader and writer.
        let (gas_tx, gas_rx) = watch::channel(GasPriceStatistics::mvm_new());
        // Start with gas unit price of 100
        let mean = OnlineMean::new(GasPriceStatistics::MVM_MIN_GAS_UNIT_PRICE);
        let tx_kind_writer = GasPriceWriter { mean, gas_tx };

        writer
            .gas_stats_per_tx_kind
            .insert(TransactionKind::MoveVm, tx_kind_writer);
        reader
            .gas_stats_per_tx_kind
            .insert(TransactionKind::MoveVm, gas_rx);
    }

    (writer, reader)
}
