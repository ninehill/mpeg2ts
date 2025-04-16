use std::collections::HashMap;
use std::io::Read;

use ts::payload::{Bytes, Null, Pat, Pes, Pmt};
use ts::{AdaptationField, Pid, TsHeader, TsPacket, TsPayload};
use {ErrorKind, Result};

/// The `ReadTsPacket` trait allows for reading TS packets from a source.
pub trait ReadTsPacket {
    /// Reads a TS packet.
    ///
    /// If the end of the stream is reached, it will return `Ok(None)`.
    fn read_ts_packet(&mut self) -> Result<Option<TsPacket>>;

    /// Peeks at next packet without pulling the packet off the buffer.
    fn peek_ts_packet(&mut self) -> Option<&TsPacket>;
}

/// TS packet reader.
#[derive(Debug)]
pub struct TsPacketReader<R> {
    peeked_packet: Option<TsPacket>,
    stream: R,
    pids: HashMap<Pid, PidKind>,
}
impl<R: Read> TsPacketReader<R> {
    /// Makes a new `TsPacketReader` instance.
    pub fn new(stream: R) -> Self {
        TsPacketReader {
            peeked_packet: None,
            stream,
            pids: HashMap::new(),
        }
    }

    /// Returns a reference to the underlaying byte stream.
    pub fn stream(&self) -> &R {
        &self.stream
    }

    /// Converts `TsPacketReader` into the underlaying byte stream `R`.
    pub fn into_stream(self) -> R {
        self.stream
    }

    fn read_next_packet(&mut self) -> Result<Option<TsPacket>> {
        let mut reader = self.stream.by_ref().take(TsPacket::SIZE as u64);
        let mut peek = [0; 1];
        let eos = track_io!(reader.read(&mut peek))? == 0;
        if eos {
            return Ok(None);
        }

        let (header, adaptation_field_control, payload_unit_start_indicator) =
            track!(TsHeader::read_from(peek.chain(&mut reader)))?;

        let adaptation_field = if adaptation_field_control.has_adaptation_field() {
            track!(AdaptationField::read_from(&mut reader))?
        } else {
            None
        };

        let payload = if adaptation_field_control.has_payload() {
            let payload = match header.pid.as_u16() {
                Pid::PAT => {
                    let pat = track!(Pat::read_from(&mut reader))?;
                    for pa in &pat.table {
                        self.pids.insert(pa.program_map_pid, PidKind::Pmt);
                    }
                    TsPayload::Pat(pat)
                }
                Pid::NULL => {
                    let null = track!(Null::read_from(&mut reader))?;
                    TsPayload::Null(null)
                }
                0x01..=0x1F | 0x1FFB => {
                    // Unknown (unsupported) packets
                    let bytes = track!(Bytes::read_from(&mut reader))?;
                    TsPayload::Raw(bytes)
                }
                _ => {
                    if !self.pids.contains_key(&header.pid) {
                        let null = track!(Null::read_from(&mut reader))?;
                        TsPayload::Null(null)
                    } else {
                        let kind = track_assert_some!(
                            self.pids.get(&header.pid).cloned(),
                            ErrorKind::InvalidInput,
                            "Unknown PID: header={:?}",
                            header
                        );
                        match kind {
                            PidKind::Pmt => {
                                let pmt = track!(Pmt::read_from(&mut reader))?;
                                for es in &pmt.table {
                                    self.pids.insert(es.elementary_pid, PidKind::Pes);
                                }
                                TsPayload::Pmt(pmt)
                            }
                            PidKind::Pes => {
                                if payload_unit_start_indicator {
                                    let pes = track!(Pes::read_from(&mut reader))?;
                                    TsPayload::Pes(pes)
                                } else {
                                    let bytes = track!(Bytes::read_from(&mut reader))?;
                                    TsPayload::Raw(bytes)
                                }
                            }
                        }
                    }
                }
            };
            Some(payload)
        } else {
            None
        };

        track_assert_eq!(reader.limit(), 0, ErrorKind::InvalidInput);
        Ok(Some(TsPacket {
            header,
            adaptation_field,
            payload,
        }))
    }

    fn get_next_available_packet(&mut self) -> Option<TsPacket> {
        let mut next_packet = None;

        loop {
            match self.read_next_packet() {
                Ok(p) => {
                    next_packet = p;
                    break;
                }
                Err(e) => {
                    log::trace!("Dropped packet: {:?}", e);
                }
            }
        }

        next_packet
    }
}
impl<R: Read> ReadTsPacket for TsPacketReader<R> {
    fn peek_ts_packet(&mut self) -> Option<&TsPacket> {
        if self.peeked_packet.is_none() {
            let next_packet = self.get_next_available_packet();
            self.peeked_packet = next_packet;
        }

        self.peeked_packet.as_ref()
    }

    fn read_ts_packet(&mut self) -> Result<Option<TsPacket>> {
        return if self.peeked_packet.is_some() {
            let packet = self.peeked_packet.to_owned();
            self.peeked_packet = None;
            Ok(packet)
        } else {
            //TODO: This is currently a bit of a hack
            Ok(self.get_next_available_packet())
        };
    }
}

#[derive(Debug, Clone)]
enum PidKind {
    Pmt,
    Pes,
}
