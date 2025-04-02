use std::{
    collections::{HashMap, VecDeque},
    env,
};

use crate::ts::{
    payload::{Bytes, Pes},
    Pid, TsPacket, TsPayload,
};
use {ErrorKind, Result};

use super::{PartialPesPacket, PesPacket};

const TS_IGNORE_HEADER_LENGTH: &str = "TS_IGNORE_HEADER_LENGTH";

/// PES packet decoder.
#[derive(Debug, Default)]
pub struct PesPacketDecoder {
    pes_packets: HashMap<Pid, PartialPesPacket>,
    back_buffer: VecDeque<PesPacket<Vec<u8>>>,
    ignore_packet_header_length: bool,
    eos: bool,
}

impl PesPacketDecoder {
    pub fn new() -> Self {
        let ignore_packet_header_length = env::var(TS_IGNORE_HEADER_LENGTH)
            .unwrap_or("false".into())
            .to_lowercase()
            == "true";
        PesPacketDecoder {
            pes_packets: HashMap::new(),
            back_buffer: VecDeque::<PesPacket<Vec<u8>>>::with_capacity(200),
            ignore_packet_header_length,
            eos: false,
        }
    }

    fn handle_eos(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if let Some(key) = self.pes_packets.keys().next().cloned() {
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
                "pes.packet_len={}, optional_header_len={}",
                pes.pes_packet_len,
                optional_header_len
            );
            Some((pes.pes_packet_len - optional_header_len) as usize)
        };

        let mut data = Vec::with_capacity(data_len.unwrap_or(pes.data.len()));
        data.extend_from_slice(&pes.data);

        let packet = PesPacket {
            header: pes.header,
            data,
        };
        let partial = PartialPesPacket { packet, data_len };
        if let Some(pred) = self.pes_packets.insert(pid, partial) {
            track_assert!(
                pred.data_len.is_none() || pred.data_len == Some(pred.packet.data.len()),
                ErrorKind::InvalidInput,
                "Unexpected PES packet"
            );
            Ok(Some(pred.packet))
        } else {
            Ok(None)
        }
    }

    fn handle_raw_payload(&mut self, pid: Pid, data: &Bytes) -> Result<Option<PesPacket<Vec<u8>>>> {
        let mut partial = match self.pes_packets.remove(&pid) {
            Some(partial) => partial,
            None => return Ok(None),
        };

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
                    return Ok(None);
                }
            }
            self.pes_packets.insert(pid, partial);
            Ok(None)
        }
    }

    pub fn process_ts_packet(
        &mut self,
        ts_packet: &TsPacket,
    ) -> Result<Option<PesPacket<Vec<u8>>>> {
        let pid = ts_packet.header.pid;
        let result = match &ts_packet.payload {
            Some(TsPayload::Pes(payload)) => track!(self.handle_pes_payload(pid, payload.clone()))?,
            Some(TsPayload::Raw(payload)) => track!(self.handle_raw_payload(pid, payload))?,
            _ => None,
        };
        Ok(result)
    }

    pub fn flush(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if self.eos {
            return track!(self.handle_eos());
        }

        self.eos = true;
        track!(self.handle_eos())
    }
}
