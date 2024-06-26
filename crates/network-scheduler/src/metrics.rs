use std::pin::Pin;
use std::time::SystemTime;

use derive_enum_from_into::EnumFrom;
use serde::Serialize;
use serde_with::{serde_as, TimestampMilliSeconds};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use subsquid_messages::{Ping, QueryFinished, QuerySubmitted};
use subsquid_network_transport::PeerId;

use crate::cli::Cli;
use crate::worker_state::WorkerState;

#[serde_as]
#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    #[serde_as(as = "TimestampMilliSeconds")]
    timestamp: SystemTime,
    #[serde(flatten)]
    event: MetricsEvent,
}

impl Metrics {
    pub fn new(peer_id: Option<String>, event: impl Into<MetricsEvent>) -> anyhow::Result<Self> {
        let event = event.into();
        let expected_sender = match &event {
            MetricsEvent::QuerySubmitted(QuerySubmitted { client_id, .. }) => Some(client_id),
            MetricsEvent::QueryFinished(QueryFinished { client_id, .. }) => Some(client_id),
            MetricsEvent::Ping(Ping { worker_id, .. }) => worker_id.as_ref(),
            _ => None,
        };
        anyhow::ensure!(
            peer_id.as_ref() == expected_sender,
            "Invalid metrics message sender"
        );

        Ok(Self {
            timestamp: SystemTime::now(),
            event,
        })
    }

    pub fn to_json_line(&self) -> anyhow::Result<Vec<u8>> {
        let mut vec = serde_json::to_vec(self)?;
        vec.push(b'\n');
        Ok(vec)
    }
}

#[derive(Debug, Clone, Serialize, EnumFrom)]
#[serde(tag = "event")]
pub enum MetricsEvent {
    Ping(Ping),
    QuerySubmitted(QuerySubmitted),
    QueryFinished(QueryFinished),
    WorkersSnapshot { active_workers: Vec<WorkerState> },
}

impl MetricsEvent {
    pub fn name(&self) -> &'static str {
        match self {
            MetricsEvent::Ping(_) => "Ping",
            MetricsEvent::QuerySubmitted(_) => "QuerySubmitted",
            MetricsEvent::QueryFinished(_) => "QueryFinished",
            MetricsEvent::WorkersSnapshot { .. } => "WorkersSnapshot",
        }
    }
}

impl From<Vec<WorkerState>> for MetricsEvent {
    fn from(active_workers: Vec<WorkerState>) -> Self {
        Self::WorkersSnapshot { active_workers }
    }
}

pub struct MetricsWriter {
    output: Pin<Box<dyn AsyncWrite + Send + Sync>>,
    enabled_metrics: Vec<String>,
}

impl MetricsWriter {
    pub async fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        let output: Pin<Box<dyn AsyncWrite + Send + Sync>> = match &cli.metrics_path {
            Some(path) => {
                let metrics_file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?;
                Box::pin(metrics_file)
            }
            None => Box::pin(tokio::io::stdout()),
        };
        let enabled_metrics = cli.metrics.clone();
        Ok(Self {
            output,
            enabled_metrics,
        })
    }

    fn metric_enabled(&self, event: &MetricsEvent) -> bool {
        let event_name = event.name();
        self.enabled_metrics.iter().any(|s| s == event_name)
    }

    pub async fn write_metrics(
        &mut self,
        peer_id: Option<PeerId>,
        msg: impl Into<MetricsEvent>,
    ) -> anyhow::Result<()> {
        let peer_id = peer_id.map(|id| id.to_string());
        let metrics = Metrics::new(peer_id, msg)?;
        if self.metric_enabled(&metrics.event) {
            let json_line = metrics.to_json_line()?;
            self.output.write_all(json_line.as_slice()).await?;
        }
        Ok(())
    }
}
