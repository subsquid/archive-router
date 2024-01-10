use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use subsquid_messages::envelope::Msg;
use subsquid_messages::signatures::SignedMessage;
use subsquid_messages::{Envelope, LogsCollected, ProstMsg, QueryLogs};
use subsquid_network_transport::transport::P2PTransportHandle;
use subsquid_network_transport::{MsgContent as MsgContentT, PeerId};

use crate::collector::LogsCollector;
use crate::storage::LogsStorage;
use crate::LOGS_TOPIC;

type MsgContent = Box<[u8]>;
type Message = subsquid_network_transport::Message<MsgContent>;

pub struct Server<T: LogsStorage + Send + Sync + 'static> {
    incoming_messages: Receiver<Message>,
    transport_handle: P2PTransportHandle<MsgContent>,
    logs_collector: Arc<RwLock<LogsCollector<T>>>,
}

impl<T: LogsStorage + Send + Sync + 'static> Server<T> {
    pub fn new(
        incoming_messages: Receiver<Message>,
        transport_handle: P2PTransportHandle<MsgContent>,
        logs_collector: LogsCollector<T>,
    ) -> Self {
        let logs_collector = Arc::new(RwLock::new(logs_collector));
        Self {
            incoming_messages,
            transport_handle,
            logs_collector,
        }
    }

    pub async fn run(mut self, store_logs_interval: Duration) -> anyhow::Result<()> {
        log::info!("Starting logs collector server");

        // Perform initial storage sync to get sequence numbers
        self.logs_collector.write().await.storage_sync().await?;

        let saving_task = self.spawn_saving_task(store_logs_interval);
        while let Some(msg) = self.incoming_messages.recv().await {
            self.handle_message(msg).await
        }
        log::info!("Server shutting down");
        saving_task.abort();
        Ok(())
    }

    async fn handle_message(&mut self, msg: Message) {
        let peer_id = match msg.peer_id {
            Some(peer_id) => peer_id,
            None => return log::warn!("Dropping anonymous message"),
        };
        let envelope = match Envelope::decode(msg.content.as_slice()) {
            Ok(envelope) => envelope,
            Err(e) => return log::warn!("Error decoding message: {e:?}"),
        };
        match envelope.msg {
            Some(Msg::QueryLogs(query_logs)) => self.collect_logs(peer_id, query_logs).await,
            _ => log::warn!("Unexpected msg received: {envelope:?}"),
        }
    }

    async fn collect_logs(
        &self,
        worker_id: PeerId,
        QueryLogs {
            mut queries_executed,
        }: QueryLogs,
    ) {
        queries_executed = queries_executed
            .into_iter()
            .filter_map(|mut log| {
                if log.verify_signature(&worker_id) {
                    Some(log)
                } else {
                    log::error!("Invalid log signature worker_id = {worker_id}");
                    None
                }
            })
            .collect();
        self.logs_collector
            .write()
            .await
            .collect_logs(worker_id, queries_executed);
    }

    fn spawn_saving_task(&self, store_logs_interval: Duration) -> JoinHandle<()> {
        let collector = self.logs_collector.clone();
        let transport_handle = self.transport_handle.clone();

        tokio::spawn(async move {
            log::info!("Starting logs saving task");
            loop {
                tokio::time::sleep(store_logs_interval).await;
                let sequence_numbers = match collector.write().await.storage_sync().await {
                    Err(e) => {
                        log::error!("Error saving logs to storage: {e:?}");
                        continue;
                    }
                    Ok(seq_nums) => seq_nums,
                };

                let msg = Msg::LogsCollected(LogsCollected { sequence_numbers });
                let envelope = Envelope { msg: Some(msg) };
                let msg_content = envelope.encode_to_vec().into();
                if let Err(e) = transport_handle
                    .broadcast_msg(msg_content, LOGS_TOPIC)
                    .await
                {
                    log::error!("Error sending message: {e:?}");
                }
            }
        })
    }
}