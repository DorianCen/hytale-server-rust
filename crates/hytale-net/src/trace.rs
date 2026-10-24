//! Packet trace capture shared by the server, the bot and the v1 gate.
//!
//! A trace records only metadata: packet id/name, channel, direction, stream,
//! order, payload length and a SHA-256 of the wire payload. Raw payload bytes
//! are never written, so a trace can be committed without redistributing
//! proprietary asset or protocol content beyond the existing fixtures.
//!
//! The server writes one JSON-lines file per run (`--trace <path>`); each line
//! is either the header or one packet record. See
//! `testdata/golden/server/trace/README.md` for the capture runbook.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use hytale_protocol::generated::registry::{packet_info, settings};
use hytale_protocol::generated::wire::NetworkChannel;

/// Current trace schema version.
pub const TRACE_FORMAT_VERSION: u32 = 1;

/// Which QUIC stream a packet used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceStream {
    /// Client-opened bidirectional setup/control stream.
    Setup,
    /// Server-opened unidirectional chunk stream.
    Chunks,
}

/// Packet direction relative to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceDirection {
    /// Client to server.
    Inbound,
    /// Server to client.
    Outbound,
}

/// One captured packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceRecord {
    /// Session number within the trace (1-based, in accept order).
    pub session: u32,
    /// Monotonic packet sequence within the session.
    pub seq: u64,
    /// Stream the packet used.
    pub stream: TraceStream,
    /// Direction relative to the server.
    pub direction: TraceDirection,
    /// Wire packet id.
    pub packet_id: u32,
    /// Java class name from the generated packet registry, when known.
    pub packet_name: Option<String>,
    /// Channel declared by the generated packet registry.
    pub channel: Option<String>,
    /// Payload as seen on the wire (after compression, when the packet id is
    /// compressed).
    pub payload_len: usize,
    /// Lowercase hex SHA-256 of the wire payload.
    pub payload_sha256: String,
}

/// Trace file header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceHeader {
    /// Producer label (`hytale-server` plus any caller note).
    pub producer: String,
    /// Protocol CRC used by this run.
    pub protocol_crc: i32,
    /// Protocol build number.
    pub protocol_build: u32,
    /// Protocol version.
    pub protocol_version: u32,
    /// Schema version.
    pub format: u32,
}

/// One JSONL trace line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceLine {
    /// First line of every trace.
    Header(TraceHeader),
    /// One packet record.
    Packet(TraceRecord),
}

/// A parsed trace file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFile {
    /// File header.
    pub header: TraceHeader,
    /// Packet records in capture order.
    pub records: Vec<TraceRecord>,
}

/// Receiver for packet metadata captured at the stream boundary.
///
/// Implementations must be cheap and non-blocking enough for the network path;
/// the JSONL recorder below appends one line under a mutex.
pub trait PacketTraceSink: Send + Sync {
    /// Record one complete frame's payload.
    fn record(
        &self,
        stream: TraceStream,
        direction: TraceDirection,
        packet_id: u32,
        payload: &[u8],
    );
}

/// Appends per-session packet records to a JSONL file.
pub struct TraceRecorder {
    producer: String,
    file: Mutex<File>,
    next_session: AtomicU32,
}

impl fmt::Debug for TraceRecorder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TraceRecorder")
            .field("producer", &self.producer)
            .field("sessions", &self.next_session.load(Ordering::Relaxed))
            .finish()
    }
}

impl TraceRecorder {
    /// Create (or truncate) a trace file and write its header.
    pub fn create(path: impl AsRef<Path>, producer: impl Into<String>) -> io::Result<Arc<Self>> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let producer = producer.into();
        let mut file = File::create(path)?;
        write_line(
            &mut file,
            &TraceLine::Header(TraceHeader {
                producer: producer.clone(),
                protocol_crc: settings::CRC,
                protocol_build: settings::BUILD_NUMBER,
                protocol_version: settings::VERSION,
                format: TRACE_FORMAT_VERSION,
            }),
        )?;
        file.flush()?;
        Ok(Arc::new(Self {
            producer,
            file: Mutex::new(file),
            next_session: AtomicU32::new(1),
        }))
    }

    /// Allocate the next session id and return its recording sink.
    pub fn begin_session(self: &Arc<Self>) -> Arc<TraceSession> {
        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        Arc::new(TraceSession {
            recorder: Arc::clone(self),
            session,
            seq: AtomicU64::new(0),
        })
    }

    /// Flush buffered records to the OS.
    pub fn flush(&self) -> io::Result<()> {
        self.file.lock().expect("trace recorder poisoned").flush()
    }
}

/// One session's writer into a shared [`TraceRecorder`].
pub struct TraceSession {
    recorder: Arc<TraceRecorder>,
    session: u32,
    seq: AtomicU64,
}

impl fmt::Debug for TraceSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TraceSession")
            .field("session", &self.session)
            .finish()
    }
}

impl TraceSession {
    /// Session number in the trace.
    pub fn session(&self) -> u32 {
        self.session
    }
}

impl PacketTraceSink for TraceSession {
    fn record(
        &self,
        stream: TraceStream,
        direction: TraceDirection,
        packet_id: u32,
        payload: &[u8],
    ) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let info = packet_info(packet_id);
        let record = TraceLine::Packet(TraceRecord {
            session: self.session,
            seq,
            stream,
            direction,
            packet_id,
            packet_name: info.map(|info| info.name.to_string()),
            channel: info.map(|info| channel_name(info.channel)),
            payload_len: payload.len(),
            payload_sha256: sha256_hex(payload),
        });
        let mut file = self.recorder.file.lock().expect("trace recorder poisoned");
        let _ = write_line(&mut file, &record);
    }
}

pub(crate) fn trace_wire_frame(
    sink: &Option<Arc<dyn PacketTraceSink>>,
    stream: TraceStream,
    direction: TraceDirection,
    frame: &[u8],
) {
    let Some(sink) = sink else {
        return;
    };
    if frame.len() < 8 {
        return;
    }
    let packet_id = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
    sink.record(stream, direction, packet_id, &frame[8..]);
}

fn write_line(file: &mut File, line: &TraceLine) -> io::Result<()> {
    serde_json::to_writer(&mut *file, line)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    file.write_all(b"\n")
}

fn channel_name(channel: NetworkChannel) -> String {
    match channel {
        NetworkChannel::Default => "Default",
        NetworkChannel::Chunks => "Chunks",
        NetworkChannel::WorldMap => "WorldMap",
        NetworkChannel::Voice => "Voice",
    }
    .to_string()
}

fn sha256_hex(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Write a normalized trace (header plus records) as JSONL.
pub fn write_trace(path: impl AsRef<Path>, trace: &TraceFile) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut file = File::create(path)?;
    write_line(&mut file, &TraceLine::Header(trace.header.clone()))?;
    for record in &trace.records {
        write_line(&mut file, &TraceLine::Packet(record.clone()))?;
    }
    file.flush()
}

/// Load a JSONL trace file written by [`TraceRecorder`].
pub fn read_trace(path: impl AsRef<Path>) -> io::Result<TraceFile> {
    let file = File::open(path)?;
    let mut header = None;
    let mut records = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: TraceLine = serde_json::from_str(&line).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("trace line {}: {error}", index + 1),
            )
        })?;
        match parsed {
            TraceLine::Header(value) if header.is_none() => header = Some(value),
            TraceLine::Header(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "trace contains more than one header",
                ));
            }
            TraceLine::Packet(record) => records.push(record),
        }
    }
    let header = header
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "trace header is missing"))?;
    Ok(TraceFile { header, records })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_round_trips_metadata_without_payloads() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hytale-trace-test-{}-{nonce}.jsonl",
            std::process::id()
        ));
        let recorder = TraceRecorder::create(&path, "test").expect("create trace");
        let session = recorder.begin_session();
        session.record(
            TraceStream::Chunks,
            TraceDirection::Outbound,
            32,
            &[1, 2, 3, 4],
        );
        recorder.flush().expect("flush trace");
        drop(session);
        drop(recorder);

        let trace = read_trace(&path).expect("read trace");
        assert_eq!(trace.header.format, TRACE_FORMAT_VERSION);
        assert_eq!(trace.header.producer, "test");
        assert_eq!(trace.records.len(), 1);
        let record = &trace.records[0];
        assert_eq!(record.session, 1);
        assert_eq!(record.seq, 0);
        assert_eq!(record.stream, TraceStream::Chunks);
        assert_eq!(record.direction, TraceDirection::Outbound);
        assert_eq!(record.packet_id, 32);
        assert_eq!(record.packet_name.as_deref(), Some("ViewRadius"));
        assert_eq!(record.channel.as_deref(), Some("Chunks"));
        assert_eq!(record.payload_len, 4);
        assert_eq!(
            record.payload_sha256,
            "9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a"
        );
        let text = std::fs::read_to_string(&path).expect("read raw trace");
        assert!(!text.contains("AQIDBA"));
        std::fs::remove_file(path).ok();
    }
}
