use crate::rpc_epoch_manager_middleware::RpcEpochManagerMiddlewareSender;
use epoch_manager::{EpochManager, Notification};
use network_manager::NetworkManager;
use notifier::{NotificationSchedule, SubscriptionCategory};
use socrypto::Identity;
use types::settings::node_identity::ConstIdentityProvider;

/// The [EpochManager] used by RPC nodes.
pub type RpcEpochManager = EpochManager<
    NetworkManager<Identity>,
    RpcEpochManagerMiddlewareSender,
    RpcEpochChangeNotificationSchedule,
    ConstIdentityProvider,
>;

/// A type that defines the order in which the various SMR components should be updated during
/// an epoch change.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialOrd, PartialEq)]
pub enum RpcEpochChangeNotificationSchedule {
    /// The only component that is currently epoch-aware in the RPC node is the [BlockListener].
    ChainStateAssembler,
    Pruner,
}

impl NotificationSchedule<Notification, RpcEpochChangeNotificationSchedule>
    for RpcEpochChangeNotificationSchedule
{
    fn get_schedule(
        _n: &Notification,
    ) -> Vec<SubscriptionCategory<RpcEpochChangeNotificationSchedule>> {
        vec![
            SubscriptionCategory::Mandatory(
                RpcEpochChangeNotificationSchedule::ChainStateAssembler,
            ),
            SubscriptionCategory::Optional(RpcEpochChangeNotificationSchedule::Pruner),
        ]
    }
}
