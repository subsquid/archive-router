use std::cmp::min;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

use aws_sdk_s3 as s3;
use aws_sdk_s3::types::Object;
use sha3::{Digest, Sha3_256};
use subsquid_network_transport::PeerId;
use tokio::sync::mpsc::{self, Receiver, Sender};

use router_controller::messages::WorkerState;
use router_controller::range::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId([u8; 32]);

impl ChunkId {
    pub fn distance(&self, peer_id: &PeerId) -> [u8; 32] {
        // Take 32 *last* bytes of the peer ID, because initial bytes of a multihash are metadata
        let peer_id = peer_id.to_bytes();
        let num_bytes = min(32, peer_id.len());
        let mut result = [0u8; 32];
        result[..num_bytes].copy_from_slice(&peer_id[(peer_id.len() - num_bytes)..]);

        // Compute XOR with chunk ID bytes
        for i in 0..32 {
            result[i] ^= self.0[i];
        }
        result
    }
}

impl Display for ChunkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        hex::encode(self.0).fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataChunk {
    dataset_url: String,
    block_range: Range,
}

impl Display for DataChunk {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format!(
            "{}/{}-{}",
            self.dataset_url, self.block_range.begin, self.block_range.end
        )
        .fmt(f)
    }
}

impl DataChunk {
    pub fn new(bucket: &str, chunk_str: &str) -> anyhow::Result<Self> {
        let chunk = router_controller::data_chunk::DataChunk::from_str(chunk_str)
            .map_err(|_| anyhow::anyhow!("Invalid chunk: {chunk_str}"))?;
        Ok(Self {
            dataset_url: format!("s3://{bucket}"),
            block_range: chunk.into(),
        })
    }

    pub fn id(&self) -> ChunkId {
        let mut result = [0u8; 32];
        let mut hasher = Sha3_256::default();
        hasher.update(self.to_string().as_bytes());
        Digest::finalize_into(hasher, result.as_mut_slice().into());
        ChunkId(result)
    }
}

pub fn chunks_to_worker_state(chunks: impl IntoIterator<Item = DataChunk>) -> WorkerState {
    let datasets = chunks
        .into_iter()
        .fold(
            HashMap::<String, Vec<Range>>::new(),
            |mut datasets, chunk| {
                datasets
                    .entry(chunk.dataset_url)
                    .or_default()
                    .push(chunk.block_range);
                datasets
            },
        )
        .into_iter()
        .map(|(dataset, ranges)| (dataset, ranges.into()))
        .collect();
    WorkerState { datasets }
}

#[derive(Clone)]
struct S3Storage {
    bucket: String,
    client: s3::Client,
}

impl S3Storage {
    pub fn new(bucket: String, client: s3::Client) -> anyhow::Result<Self> {
        Ok(Self { bucket, client })
    }

    async fn list_objects(
        &self,
        last_key: Option<String>,
    ) -> anyhow::Result<Option<(String, Vec<Object>)>> {
        let s3_result = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .set_start_after(last_key)
            .send()
            .await?;

        let objects = match s3_result.contents {
            Some(contents) if !contents.is_empty() => contents,
            _ => return Ok(None),
        };

        let last_key = objects
            .last()
            .unwrap()
            .key()
            .ok_or_else(|| anyhow::anyhow!("Object key missing"))?
            .to_string();

        return Ok(Some((last_key, objects)));
    }

    fn obj_to_chunk(&self, obj: Object) -> Option<DataChunk> {
        let key = match obj.key() {
            Some(key) if key.ends_with("blocks.parquet") => key.to_string(),
            _ => return None,
        };
        let chunk = match DataChunk::new(&self.bucket, &key) {
            Ok(chunk) => chunk,
            Err(_) => {
                log::error!("Invalid data chunk: {}", key);
                return None;
            }
        };
        Some(chunk)
    }

    pub async fn send_chunks(self, sender: Sender<DataChunk>) {
        log::info!("Reading chunks from bucket {}", self.bucket);
        let mut last_key = None;
        loop {
            let objects = match self.list_objects(last_key.clone()).await {
                Ok(Some((key, objects))) => {
                    last_key = Some(key);
                    objects
                }
                Ok(None) => {
                    log::info!("Now more chunks for now. Waiting...");
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
                Err(e) => {
                    log::error!("Error listing objects: {e:?}");
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
            };

            let chunks: Vec<DataChunk> = objects
                .into_iter()
                .filter_map(|obj| self.obj_to_chunk(obj))
                .collect();
            log::info!(
                "Downloaded {} new chunks from bucket {}",
                chunks.len(),
                self.bucket
            );
            for chunk in chunks {
                match sender.send(chunk).await {
                    Err(_) => return,
                    Ok(_) => continue,
                }
            }
        }
    }
}

pub async fn get_incoming_chunks(
    s3_endpoint: String,
    buckets: Vec<String>,
) -> anyhow::Result<Receiver<DataChunk>> {
    let config = aws_config::from_env()
        .endpoint_url(s3_endpoint)
        .load()
        .await;
    let client = s3::Client::new(&config);

    let (sender, receiver) = mpsc::channel(1024);

    for bucket in buckets {
        let storage = S3Storage::new(bucket, client.clone())?;
        let sender = sender.clone();
        tokio::spawn(storage.send_chunks(sender));
    }
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    use subsquid_network_transport::PeerId;

    use router_controller::range::RangeSet;

    use super::*;

    #[test]
    fn test_chunk() {
        let chunk = DataChunk {
            dataset_url: "s3://squidnet".to_string(),
            block_range: Range::new(0, 1000),
        };
        assert_eq!(chunk.to_string(), "s3://squidnet/0-1000");
        assert_eq!(
            chunk.id(),
            ChunkId([
                0xcd, 0x62, 0xa3, 0xf1, 0xf2, 0x48, 0x5b, 0x3d, 0x88, 0x0d, 0x77, 0x03, 0x75, 0xe4,
                0x52, 0x3e, 0x63, 0x8f, 0x08, 0xdf, 0xf0, 0x98, 0x89, 0x9c, 0xbf, 0xbc, 0x02, 0xf3,
                0x52, 0xea, 0x53, 0x90
            ])
        );
    }

    #[test]
    fn test_distance() {
        let peer_id: PeerId = "12D3KooWQER7HEpwsvqSzqzaiV36d3Bn6DZrnwEunnzS76pgZkMU"
            .parse()
            .unwrap();
        let chunk = DataChunk {
            dataset_url: "s3://squidnet".to_string(),
            block_range: Range::new(0, 1000),
        };
        assert_eq!(
            chunk.id().distance(&peer_id),
            [
                0x1b, 0x4e, 0x42, 0x76, 0x72, 0x9c, 0x4b, 0xab, 0x9f, 0x86, 0x7c, 0xc3, 0xb4, 0xcd,
                0xe6, 0x01, 0x05, 0x18, 0x16, 0x62, 0x8b, 0x1d, 0x00, 0x25, 0x41, 0x2f, 0x8a, 0xe0,
                0xed, 0xa9, 0xc2, 0x2f
            ]
        )
    }

    #[test]
    #[rustfmt::skip]
    fn test_worker_state() {
        let chunks = vec![
            DataChunk {
                dataset_url: "s3://squidnet".to_string(),
                block_range: Range::new(0, 1000),
            },
            DataChunk {
                dataset_url: "s3://squidnet".to_string(),
                block_range: Range::new(500, 1500),
            },
            DataChunk {
                dataset_url: "s3://pepenet".to_string(),
                block_range: Range::new(1234, 5678),
            },
        ];

        assert_eq!(chunks_to_worker_state(chunks), WorkerState {
            datasets: vec![
                ("s3://squidnet".to_string(), RangeSet { ranges: vec![Range::new(0, 1500)] }),
                ("s3://pepenet".to_string(), RangeSet { ranges: vec![Range::new(1234, 5678)] }),
            ].into_iter().collect()
        })
    }
}