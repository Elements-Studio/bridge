// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The StarcoinSyncer module is responsible for synchronizing Events emitted
//! on Starcoin blockchain from concerned modules of bridge package 0x9.

use crate::{
    error::BridgeResult,
    metrics::BridgeMetrics,
    retry_with_max_elapsed_time,
    starcoin_bridge_client::{StarcoinClient, StarcoinClientInner},
};
use mysten_metrics::spawn_logged_monitored_task;
use std::{collections::HashMap, sync::Arc};
use starcoin_bridge_json_rpc_types::StarcoinEvent;
use starcoin_bridge_types::BRIDGE_PACKAGE_ID;
use starcoin_bridge_types::{event::EventID, Identifier};
use tokio::{
    sync::Notify,
    task::JoinHandle,
    time::{self, Duration},
};

const STARCOIN_EVENTS_CHANNEL_SIZE: usize = 1000;

// Map from contract address to their start cursor (exclusive)
pub type StarcoinTargetModules = HashMap<Identifier, Option<EventID>>;

pub struct StarcoinSyncer<C> {
    starcoin_bridge_client: Arc<StarcoinClient<C>>,
    // The last transaction that the syncer has fully processed.
    // Syncer will resume post this transaction (i.e. exclusive), when it starts.
    cursors: StarcoinTargetModules,
    metrics: Arc<BridgeMetrics>,
}

impl<C> StarcoinSyncer<C>
where
    C: StarcoinClientInner + 'static,
{
    pub fn new(
        starcoin_bridge_client: Arc<StarcoinClient<C>>,
        cursors: StarcoinTargetModules,
        metrics: Arc<BridgeMetrics>,
    ) -> Self {
        Self {
            starcoin_bridge_client,
            cursors,
            metrics,
        }
    }

    pub async fn run(
        self,
        query_interval: Duration,
    ) -> BridgeResult<(
        Vec<JoinHandle<()>>,
        mysten_metrics::metered_channel::Receiver<(Identifier, Vec<StarcoinEvent>)>,
    )> {
        let (events_tx, events_rx) = mysten_metrics::metered_channel::channel(
            STARCOIN_EVENTS_CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["starcoin_bridge_events_queue"]),
        );

        let mut task_handles = vec![];
        for (module, cursor) in self.cursors {
            let metrics = self.metrics.clone();
            let events_rx_clone: mysten_metrics::metered_channel::Sender<(
                Identifier,
                Vec<StarcoinEvent>,
            )> = events_tx.clone();
            let starcoin_bridge_client_clone = self.starcoin_bridge_client.clone();
            task_handles.push(spawn_logged_monitored_task!(
                Self::run_event_listening_task(
                    module,
                    cursor,
                    events_rx_clone,
                    starcoin_bridge_client_clone,
                    query_interval,
                    metrics,
                )
            ));
        }
        Ok((task_handles, events_rx))
    }

    async fn run_event_listening_task(
        // The module where interested events are defined.
        // Module is always of bridge package 0x9.
        module: Identifier,
        initial_cursor: Option<EventID>,
        events_sender: mysten_metrics::metered_channel::Sender<(Identifier, Vec<StarcoinEvent>)>,
        starcoin_bridge_client: Arc<StarcoinClient<C>>,
        query_interval: Duration,
        metrics: Arc<BridgeMetrics>,
    ) {
        // Convert EventID to cursor string for pagination
        let mut cursor = initial_cursor;
        tracing::info!(?module, ?cursor, "Starting starcoin events listening task");
        let mut interval = time::interval(query_interval);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        // Create a task to update metrics
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let starcoin_bridge_client_clone = starcoin_bridge_client.clone();
        let last_synced_starcoin_bridge_checkpoints_metric = metrics
            .last_synced_starcoin_bridge_checkpoints
            .with_label_values(&[&module.to_string()]);
        spawn_logged_monitored_task!(async move {
            loop {
                notify_clone.notified().await;
                let Ok(Ok(latest_checkpoint_sequence_number)) = retry_with_max_elapsed_time!(
                    starcoin_bridge_client_clone.get_latest_checkpoint_sequence_number(),
                    Duration::from_secs(120)
                ) else {
                    tracing::error!("Failed to query latest checkpoint sequence number from starcoin client after retry");
                    continue;
                };
                last_synced_starcoin_bridge_checkpoints_metric.set(latest_checkpoint_sequence_number as i64);
            }
        });

        loop {
            interval.tick().await;
            let Ok(Ok(events)) = retry_with_max_elapsed_time!(
                starcoin_bridge_client.query_events_by_module(BRIDGE_PACKAGE_ID, module.clone(), cursor),
                Duration::from_secs(120)
            ) else {
                tracing::error!("Failed to query events from starcoin client after retry");
                continue;
            };

            let len = events.data.len();
            if len != 0 {
                if !events.has_next_page {
                    // If this is the last page, it means we have processed all events up to the latest checkpoint
                    // We can then update the latest checkpoint metric.
                    notify.notify_one();
                }
                events_sender
                    .send((module.clone(), events.data.clone()))
                    .await
                    .expect("All Starcoin event channel receivers are closed");
                // Update cursor from last event
                if let Some(last_event) = events.data.last() {
                    cursor = Some(last_event.id.clone().into());
                }
                tracing::info!(?module, ?cursor, "Observed {len} new Starcoin events");
            }
        }
    }
}

/*#[cfg(test)]
mod tests {
    use super::*;

    use crate::{starcoin_bridge_client::StarcoinClient, starcoin_bridge_mock_client::StarcoinMockClient};
    use prometheus::Registry;
    use starcoin_bridge_json_rpc_types::EventPage;
    use starcoin_bridge_types::{digests::TransactionDigest, event::EventID, Identifier};
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_starcoin_bridge_syncer_basic() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let metrics = Arc::new(BridgeMetrics::new(&registry));
        let mock = StarcoinMockClient::default();
        let client = Arc::new(StarcoinClient::new_for_testing(mock.clone()));
        let module_foo = Identifier::new("Foo").unwrap();
        let module_bar = Identifier::new("Bar").unwrap();
        let empty_events = EventPage::empty();
        let cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 0,
        };
        add_event_response(&mock, module_foo.clone(), cursor, empty_events.clone());
        add_event_response(&mock, module_bar.clone(), cursor, empty_events.clone());

        let target_modules = HashMap::from_iter(vec![
            (module_foo.clone(), Some(cursor)),
            (module_bar.clone(), Some(cursor)),
        ]);
        let interval = Duration::from_millis(200);
        let (_handles, mut events_rx) = StarcoinSyncer::new(client, target_modules, metrics.clone())
            .run(interval)
            .await
            .unwrap();

        // Initially there are no events
        assert_no_more_events(interval, &mut events_rx).await;

        mock.set_latest_checkpoint_sequence_number(999);
        // Module Foo has new events
        let mut event_1: StarcoinEvent = StarcoinEvent::random_for_testing();
        let package_id = BRIDGE_PACKAGE_ID;
        event_1.type_.address = package_id.into();
        event_1.type_.module = module_foo.clone();
        let module_foo_events_1: starcoin_bridge_json_rpc_types::Page<StarcoinEvent, EventID> = EventPage {
            data: vec![event_1.clone(), event_1.clone()],
            next_cursor: Some(event_1.id),
            has_next_page: false,
        };
        add_event_response(&mock, module_foo.clone(), event_1.id, empty_events.clone());
        add_event_response(
            &mock,
            module_foo.clone(),
            cursor,
            module_foo_events_1.clone(),
        );

        let (identifier, received_events) = events_rx.recv().await.unwrap();
        assert_eq!(identifier, module_foo);
        assert_eq!(received_events.len(), 2);
        assert_eq!(received_events[0].id, event_1.id);
        assert_eq!(received_events[1].id, event_1.id);
        // No more
        assert_no_more_events(interval, &mut events_rx).await;
        assert_eq!(
            metrics
                .last_synced_starcoin_bridge_checkpoints
                .get_metric_with_label_values(&["Foo"])
                .unwrap()
                .get(),
            999
        );

        // Module Bar has new events
        let mut event_2: StarcoinEvent = StarcoinEvent::random_for_testing();
        event_2.type_.address = package_id.into();
        event_2.type_.module = module_bar.clone();
        let module_bar_events_1 = EventPage {
            data: vec![event_2.clone()],
            next_cursor: Some(event_2.id),
            has_next_page: true, // Set to true so that the syncer will not update the last synced checkpoint
        };
        add_event_response(&mock, module_bar.clone(), event_2.id, empty_events.clone());

        add_event_response(&mock, module_bar.clone(), cursor, module_bar_events_1);

        let (identifier, received_events) = events_rx.recv().await.unwrap();
        assert_eq!(identifier, module_bar);
        assert_eq!(received_events.len(), 1);
        assert_eq!(received_events[0].id, event_2.id);
        // No more
        assert_no_more_events(interval, &mut events_rx).await;
        assert_eq!(
            metrics
                .last_synced_starcoin_bridge_checkpoints
                .get_metric_with_label_values(&["Bar"])
                .unwrap()
                .get(),
            0, // Not updated
        );

        Ok(())
    }

    async fn assert_no_more_events(
        interval: Duration,
        events_rx: &mut mysten_metrics::metered_channel::Receiver<(Identifier, Vec<StarcoinEvent>)>,
    ) {
        match timeout(interval * 2, events_rx.recv()).await {
            Err(_e) => (),
            other => panic!("Should have timed out, but got: {:?}", other),
        };
    }

    fn add_event_response(
        mock: &StarcoinMockClient,
        module: Identifier,
        cursor: EventID,
        events: EventPage,
    ) {
        mock.add_event_response(BRIDGE_PACKAGE_ID, module.clone(), cursor, events.clone());
    }
}*/
