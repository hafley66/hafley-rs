//! Reusable, zero-model replay seam for recorded byte schedules.
//!
//! A schedule is the asciicast v2 tuple shape retained by
//! `instant/scripts/0_terminalCast.ts:10-14`: `[time, code, data]`, with time
//! relative to replay start and codes split by channel. This module parses that
//! shape into typed, byte-exact events, orders them by `(at_ms, source_index)`,
//! and drives them over any pair of `std::io::Write`/`std::io::Read` channels.
//!
//! It is not a terminal emulator. It preserves recorded bytes and keeps input,
//! output, markers and resizes on distinct channels; it makes no screen claim.

use std::io::{self, Read, Write};

/// One scheduled replay event. `data` is byte-exact for byte channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayEvent {
    /// Milliseconds relative to replay start.
    pub at_ms: u64,
    /// Zero-based source line index, the tie-break when two events share a time.
    pub source_index: usize,
    pub channel: ReplayChannel,
    pub data: Vec<u8>,
}

/// The channel a recorded event belongs to. Markers and resizes are typed
/// facts, not opaque byte payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayChannel {
    /// Bytes written into the replayed process.
    Input,
    /// Bytes the replayed process is recorded to have produced.
    Output,
    /// A named marker; `data` is its name. `replay-eof` closes input.
    Marker,
    /// A terminal geometry fact, e.g. the cast `"80x24"` payload.
    Resize { cols: u16, rows: u16 },
}

/// A rejected schedule line, with the evidence needed to reproduce the
/// rejection: 1-based line number and the offending text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayError {
    pub line: usize,
    pub evidence: String,
    pub detail: String,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "replay line {}: {} ({})",
            self.line, self.detail, self.evidence
        )
    }
}

impl std::error::Error for ReplayError {}

fn reject(line: usize, evidence: &str, detail: &str) -> ReplayError {
    ReplayError {
        line,
        evidence: evidence.to_owned(),
        detail: detail.to_owned(),
    }
}

fn parse_resize(line: usize, evidence: &str, text: &str) -> Result<ReplayChannel, ReplayError> {
    let (cols, rows) = text
        .split_once('x')
        .ok_or_else(|| reject(line, evidence, "resize payload is not COLSxROWS"))?;
    let cols = cols
        .parse::<u16>()
        .map_err(|_| reject(line, evidence, "resize cols is not a u16"))?;
    let rows = rows
        .parse::<u16>()
        .map_err(|_| reject(line, evidence, "resize rows is not a u16"))?;
    Ok(ReplayChannel::Resize { cols, rows })
}

/// Parse an asciicast v2 document into a schedule sorted by
/// `(at_ms, source_index)`. Times are relative seconds from replay start.
///
/// Malformed or unsupported lines are rejected, not dropped: the returned
/// [`ReplayError`] carries the line number and text.
pub fn parse_cast(cast: &str) -> Result<Vec<ReplayEvent>, ReplayError> {
    let mut lines = cast.lines();
    let header_line = lines.next().unwrap_or("");
    let header: serde_json::Value = serde_json::from_str(header_line)
        .map_err(|_| reject(1, header_line, "cast header is not JSON"))?;
    if header["version"].as_u64() != Some(2) {
        return Err(reject(1, header_line, "unsupported cast version"));
    }

    let mut events = Vec::new();
    for (index, line) in lines.enumerate() {
        let line_number = index + 2;
        let fields: Vec<serde_json::Value> = serde_json::from_str(line)
            .map_err(|_| reject(line_number, line, "cast event is not a JSON array"))?;
        if fields.len() != 3 {
            return Err(reject(
                line_number,
                line,
                "cast event is not a 3-tuple [time, code, data]",
            ));
        }
        let seconds = fields[0]
            .as_f64()
            .ok_or_else(|| reject(line_number, line, "event time is not a number"))?;
        if seconds < 0.0 {
            return Err(reject(
                line_number,
                line,
                "event time is negative, not relative to replay start",
            ));
        }
        let code = fields[1]
            .as_str()
            .and_then(|value| {
                let mut chars = value.chars();
                let first = chars.next()?;
                chars.next().is_none().then_some(first)
            })
            .ok_or_else(|| reject(line_number, line, "event code is not one char"))?;
        let data = fields[2]
            .as_str()
            .ok_or_else(|| reject(line_number, line, "event data is not a string"))?;
        let channel = match code {
            'i' => ReplayChannel::Input,
            'o' => ReplayChannel::Output,
            'm' => ReplayChannel::Marker,
            'r' => parse_resize(line_number, line, data)?,
            _ => return Err(reject(line_number, line, "unsupported event code")),
        };
        events.push(ReplayEvent {
            at_ms: (seconds * 1_000.0).round() as u64,
            source_index: index,
            channel,
            data: data.as_bytes().to_vec(),
        });
    }
    events.sort_by_key(|event| (event.at_ms, event.source_index));
    Ok(events)
}

/// What one driven schedule observed. Input and recorded output stay on their
/// own fields; markers and resizes are typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub input: Vec<u8>,
    pub recorded_output: Vec<u8>,
    pub actual_output: Vec<u8>,
    pub markers: Vec<(u64, String)>,
    pub resizes: Vec<(u64, u16, u16)>,
    /// True once the input channel was dropped, whether at the EOF marker or
    /// after the last event.
    pub input_closed: bool,
}

/// Drive a parsed schedule over byte channels. `input` events are written and
/// recorded; `output` events are recorded as the expected stream; a marker
/// named `eof_marker` drops the input channel early. After the schedule, input
/// is always dropped and `output` is read to end.
///
/// No wall clock is consulted: scheduling is the parsed order alone.
pub fn drive<W: Write, R: Read + Send>(
    events: &[ReplayEvent],
    input: &mut Option<W>,
    output: &mut R,
    eof_marker: &str,
) -> io::Result<ReplayOutcome> {
    // Drain concurrently: a child can fill stdout before consuming all stdin.
    // Closing input on every write outcome lets an EOF-driven child finish.
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut bytes = Vec::new();
            output.read_to_end(&mut bytes).map(|_| bytes)
        });
        let mut outcome = ReplayOutcome::default();
        let written = (|| -> io::Result<()> {
            for event in events {
                match &event.channel {
                    ReplayChannel::Input => {
                        let writer = input
                            .as_mut()
                            .ok_or_else(|| io::Error::other("input event after replay EOF"))?;
                        writer.write_all(&event.data)?;
                        outcome.input.extend_from_slice(&event.data);
                    }
                    ReplayChannel::Output => outcome.recorded_output.extend_from_slice(&event.data),
                    ReplayChannel::Marker => {
                        let name = String::from_utf8_lossy(&event.data).into_owned();
                        outcome.markers.push((event.at_ms, name.clone()));
                        if name == eof_marker {
                            input.take();
                        }
                    }
                    ReplayChannel::Resize { cols, rows } => {
                        outcome.resizes.push((event.at_ms, *cols, *rows));
                    }
                }
            }
            Ok(())
        })();
        input.take();
        outcome.input_closed = true;
        let read = reader
            .join()
            .map_err(|_| io::Error::other("replay reader panicked"))?;
        written?;
        outcome.actual_output = read?;
        Ok(outcome)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cast(events: &str) -> String {
        format!("{{\"version\":2,\"width\":80,\"height\":24}}\n{events}")
    }

    fn drive_to_vecs(events: &[ReplayEvent]) -> ReplayOutcome {
        let mut sink: Option<Vec<u8>> = Some(Vec::new());
        let mut output: &[u8] = b"";
        drive(events, &mut sink, &mut output, "replay-eof").expect("drive")
    }

    #[test]
    fn output_chunks_split_across_events_reassemble_in_schedule_order() {
        let events = parse_cast(&cast(
            "[0.000,\"o\",\"alpha\"]\n[0.100,\"o\",\"-beta\"]\n[0.200,\"o\",\"-gamma\"]\n",
        ))
        .unwrap();
        let outcome = drive_to_vecs(&events);
        assert_eq!(outcome.recorded_output, b"alpha-beta-gamma");
        assert_eq!(
            events.iter().map(|e| e.at_ms).collect::<Vec<_>>(),
            vec![0, 100, 200]
        );
    }

    #[test]
    fn equal_timestamps_break_on_source_line_index() {
        let events = parse_cast(&cast(
            "[0.100,\"o\",\"first\"]\n[0.100,\"o\",\"second\"]\n[0.050,\"o\",\"zero\"]\n",
        ))
        .unwrap();
        assert_eq!(
            events
                .iter()
                .map(|e| (e.at_ms, e.source_index, e.data.clone()))
                .collect::<Vec<_>>(),
            vec![
                (50, 2, b"zero".to_vec()),
                (100, 0, b"first".to_vec()),
                (100, 1, b"second".to_vec()),
            ]
        );
        assert_eq!(drive_to_vecs(&events).recorded_output, b"zerofirstsecond");
    }

    #[test]
    fn empty_payload_round_trips_and_channels_stay_separate() {
        let events = parse_cast(&cast(
            "[0.000,\"i\",\"\"]\n[0.000,\"o\",\"\"]\n[0.000,\"m\",\"replay-eof\"]\n",
        ))
        .unwrap();
        let outcome = drive_to_vecs(&events);
        assert!(outcome.input.is_empty());
        assert!(outcome.recorded_output.is_empty());
        assert_eq!(outcome.markers, vec![(0, "replay-eof".to_owned())]);
        assert!(outcome.input_closed);
    }

    #[test]
    fn markers_and_resizes_are_typed_facts() {
        let events = parse_cast(&cast(
            "[0.000,\"m\",\"replay-start\"]\n[0.000,\"r\",\"120x40\"]\n[0.100,\"r\",\"80x24\"]\n",
        ))
        .unwrap();
        let outcome = drive_to_vecs(&events);
        assert_eq!(outcome.markers, vec![(0, "replay-start".to_owned())]);
        assert_eq!(outcome.resizes, vec![(0, 120, 40), (100, 80, 24)]);
    }

    #[test]
    fn malformed_and_unsupported_lines_are_rejected_with_evidence() {
        let cases = [
            (
                "version 9 header",
                "{\"version\":9}\n[0.0,\"o\",\"x\"]\n",
                1,
            ),
            ("unknown code", &cast("[0.0,\"z\",\"x\"]\n"), 2),
            ("negative time", &cast("[-1.0,\"o\",\"x\"]\n"), 2),
            ("two-tuple", &cast("[0.0,\"o\"]\n"), 2),
            ("non-numeric time", &cast("[soon,\"o\",\"x\"]\n"), 2),
            ("bad resize", &cast("[0.0,\"r\",\"wide\"]\n"), 2),
        ];
        for (name, document, line) in cases {
            let error = parse_cast(document).expect_err(name);
            assert_eq!(error.line, line, "{name}");
            assert!(!error.evidence.is_empty(), "{name}");
            assert!(!error.detail.is_empty(), "{name}");
        }
    }

    #[test]
    fn input_after_eof_marker_is_rejected() {
        let events = parse_cast(&cast(
            "[0.000,\"m\",\"replay-eof\"]\n[0.100,\"i\",\"late\"]\n",
        ))
        .unwrap();
        let mut sink: Option<Vec<u8>> = Some(Vec::new());
        let mut output: &[u8] = b"";
        let error = drive(&events, &mut sink, &mut output, "replay-eof").expect_err("late input");
        assert_eq!(error.kind(), io::ErrorKind::Other);
    }

    #[cfg(unix)]
    #[test]
    fn replay_drains_output_while_input_exceeds_channel_capacity() {
        use std::os::unix::net::UnixStream;
        use std::time::Duration;
        let (local, mut peer) = UnixStream::pair().unwrap();
        for socket in [&local, &peer] {
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        let bytes = vec![b'x'; 1024 * 1024];
        let expected = bytes.clone();
        let peer = std::thread::spawn(move || {
            peer.write_all(&expected).unwrap();
            let mut received = vec![0; expected.len()];
            peer.read_exact(&mut received).unwrap();
            assert_eq!(received, expected);
        });
        let mut input = Some(local.try_clone().unwrap());
        let mut output = local;
        let outcome = drive(
            &[ReplayEvent {
                at_ms: 0,
                source_index: 0,
                channel: ReplayChannel::Input,
                data: bytes.clone(),
            }],
            &mut input,
            &mut output,
            "eof",
        )
        .unwrap();
        peer.join().unwrap();
        assert_eq!(outcome.input, bytes);
        assert_eq!(outcome.actual_output, bytes);
        assert!(outcome.input_closed);
    }
}
