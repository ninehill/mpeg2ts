use std::collections::{HashMap, VecDeque};
use std::env;

use pes::PesPacket;
use ts::payload::{Bytes, Pes};
use ts::{Pid, ReadTsPacket, TsPayload};
use {ErrorKind, Result};

const TS_IGNORE_HEADER_LENGTH : &str = "TS_IGNORE_HEADER_LENGTH";

/// The `ReadPesPacket` trait allows for reading PES packets from a source.
pub trait ReadPesPacket {
    /// Reads a PES packet.
    ///
    /// If the end of the stream is reached, it will return `Ok(None)`.
    fn read_pes_packet(&mut self) -> Result<Option<PesPacket<Vec<u8>>>>;

    ///Peeks a PES Packet
    fn peek_pes_packet(&mut self) -> Option<&PesPacket<Vec<u8>>>;

    ///Marks the reader to a spot that can be returned to at a later time
    fn mark(&mut self) -> Result<()>;

    ///Resets the reader to a previously marked spot.
    fn reset(&mut self) -> Result<()>;

    fn has_back_buffer(&self) -> bool;
}

/// PES packet reader.
#[derive(Debug)]
pub struct PesPacketReader<R> {
    peeked_packet: Option<PesPacket<Vec<u8>>>,
    ts_packet_reader: R,
    pes_packets: HashMap<Pid, PartialPesPacket>,
    eos: bool,
    is_marked: bool,
    back_buffer: VecDeque<PesPacket<Vec<u8>>>,
    ignore_packet_header_length: bool,
}
impl<R: ReadTsPacket> PesPacketReader<R> {
    /// Makes a new `PesPacketReader` instance.
    pub fn new(ts_packet_reader: R) -> Self {
        let ignore_header_length = env::var(TS_IGNORE_HEADER_LENGTH.to_string()).unwrap_or("false".into()).to_lowercase() == "true";
        PesPacketReader {
            peeked_packet: None,
            ts_packet_reader,
            pes_packets: HashMap::new(),
            eos: false,
            is_marked: false,
            back_buffer: VecDeque::<PesPacket<Vec<u8>>>::with_capacity(200),
            ignore_packet_header_length: ignore_header_length
        }
    }

    /// Returns a reference to the underlaying TS packet reader.
    pub fn ts_packet_reader(&self) -> &R {
        &self.ts_packet_reader
    }

    /// Converts `PesPacketReader` into the underlaying TS packet reader.
    pub fn into_ts_packet_reader(self) -> R {
        self.ts_packet_reader
    }

    fn handle_eos(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if let Some(key) = self.pes_packets.keys().nth(0).cloned() {
            let partial = self.pes_packets.remove(&key).expect("Never fails");
            track_assert!(
                partial.data_len.is_none() || partial.data_len == Some(partial.packet.data.len()),
                ErrorKind::InvalidInput,
                "Unexpected EOS"
            );
            Ok(Some(partial.packet))
        } else {
            Ok(None)
        }
    }

    fn handle_pes_payload(&mut self, pid: Pid, pes: Pes) -> Result<Option<PesPacket<Vec<u8>>>> {
        let data_len = if self.ignore_packet_header_length || pes.pes_packet_len == 0 {
            None
        } else {
            let optional_header_len = pes.header.optional_header_len();
            track_assert!(
                pes.pes_packet_len >= optional_header_len,
                ErrorKind::InvalidInput,
                "pes.pes_packet_len={}, optional_header_len={}",
                pes.pes_packet_len,
                optional_header_len
            );
            Some((pes.pes_packet_len - optional_header_len) as usize)
        };

        let mut data = Vec::with_capacity(data_len.unwrap_or_else(|| pes.data.len()));
        data.extend_from_slice(&pes.data);

        let packet = PesPacket {
            header: pes.header,
            data,
        };
        let partial = PartialPesPacket { packet, data_len };
        
        if let Some(pred) = self.pes_packets.insert(pid, partial) {
            Ok(Some(pred.packet))
            // if pred.data_len.is_none() || pred.data_len == Some(pred.packet.data.len()) {
            //     Ok(Some(pred.packet))
            // } else {
            //     log::trace!(
            //         "Mismatched PES packet data length: actual={}, expected={}",
            //         pred.data_len.expect("Never fails"),
            //         pred.packet.data.len()
            //     );
            //     Ok(None)
            // }
        } else {
            Ok(None)
        }
    }

    fn handle_raw_payload(&mut self, pid: Pid, data: &Bytes) -> Result<Option<PesPacket<Vec<u8>>>> {
        let possible_partial = self.pes_packets.remove(&pid);
        if possible_partial.is_none() {
            return Ok(None);
        }

        let mut partial = possible_partial.unwrap();

        partial.packet.data.extend_from_slice(data);
        if Some(partial.packet.data.len()) == partial.data_len {
            Ok(Some(partial.packet))
        } else {
            if let Some(expected) = partial.data_len {
                if partial.packet.data.len() > expected {
                   log::trace!(
                        "Too large PES packet data: actual={}, expected={}",
                        partial.packet.data.len(),
                        expected
                    );
                    return Ok(None)
                } 
            }
            self.pes_packets.insert(pid, partial);
            Ok(None)
        }
    }

    fn read_next_pes_packet(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if self.eos {
            return track!(self.handle_eos());
        }

        while let Some(ts_packet) = track!(self.ts_packet_reader.read_ts_packet())? {
            let pid = ts_packet.header.pid;
            let result = match ts_packet.payload {
                Some(TsPayload::Pes(payload)) => track!(self.handle_pes_payload(pid, payload))?,
                Some(TsPayload::Raw(payload)) => track!(self.handle_raw_payload(pid, &payload))?,
                _ => None,
            };
            if result.is_some() {
                return Ok(result);
            }
        }

        self.eos = true;
        track!(self.handle_eos())
    }
}
impl<R: ReadTsPacket> ReadPesPacket for PesPacketReader<R> {
    fn peek_pes_packet(&mut self) -> Option<&PesPacket<Vec<u8>>> {
        if self.peeked_packet.is_none() {
            self.peeked_packet = self.read_pes_packet().unwrap_or(None);
        }

        self.peeked_packet.as_ref()
    }

    fn read_pes_packet(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        let packet = if self.peeked_packet.is_some() {
            let packet = self.peek_pes_packet().unwrap().to_owned();
            self.peeked_packet = None;
            Some(packet)
        } else if !self.back_buffer.is_empty() && !self.is_marked {
            self.back_buffer.pop_front()
        } else {
            self.read_next_pes_packet()?
        };

        if self.is_marked && packet.is_some() {
            self.back_buffer
                .push_back(packet.clone().expect("never fails"));
        }

        Ok(packet)
    }

    fn mark(&mut self) -> Result<()> {
        track_assert!(!self.is_marked, ErrorKind::Other, "Reader already marked");

        self.is_marked = true;

        if self.peeked_packet.is_some() {
            let p = self.peeked_packet.clone().unwrap();
            self.back_buffer.push_back(p);
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        track_assert!(self.is_marked, ErrorKind::Other, "Reader not marked");
        self.is_marked = false;
        self.peeked_packet = None;
        Ok(())
    }

    fn has_back_buffer(&self) -> bool {
        !self.back_buffer.is_empty()
    }
}

#[derive(Debug)]
struct PartialPesPacket {
    packet: PesPacket<Vec<u8>>,
    data_len: Option<usize>,
}
