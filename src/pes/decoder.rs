use std::{collections::HashMap, env};

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
    ignore_packet_header_length: bool,
    eos: bool,
}

impl PesPacketDecoder {
    /// Creates a new `PesPacketDecoder` instance.
    pub fn new() -> Self {
        let ignore_packet_header_length = env::var(TS_IGNORE_HEADER_LENGTH)
            .unwrap_or("false".into())
            .to_lowercase()
            == "true";
        PesPacketDecoder {
            pes_packets: HashMap::new(),
            ignore_packet_header_length,
            eos: false,
        }
    }

    /// Handles end-of-stream (EOS) condition by returning any partial data.
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

    /// Handles PES payload data.
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
            Ok(Some(pred.packet))
        } else {
            Ok(None)
        }
    }

    /// Handles raw payload data.
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

    /// Processes a TS packet and returns a PES packet if available.
    pub fn process_ts_packet(
        &mut self,
        ts_packet: &TsPacket,
    ) -> Result<Option<PesPacket<Vec<u8>>>> {
        if self.eos {
            return track!(self.handle_eos());
        }

        let pid = ts_packet.header.pid;
        let result = match &ts_packet.payload {
            Some(TsPayload::Pes(payload)) => track!(self.handle_pes_payload(pid, payload.clone()))?,
            Some(TsPayload::Raw(payload)) => track!(self.handle_raw_payload(pid, payload))?,
            _ => None,
        };
        Ok(result)
    }

    /// Flush the decoder.
    pub fn flush(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if self.eos {
            return track!(self.handle_eos());
        }

        self.eos = true;
        track!(self.handle_eos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        es::StreamId,
        pes::PesHeader,
        ts::{ContinuityCounter, TransportScramblingControl, TsHeader},
    };

    #[test]
    fn test_pes_packet_decoder_raw_payload() {
        let mut decoder = PesPacketDecoder::new();
        let packet = TsPacket {
            header: TsHeader {
                transport_error_indicator: false,
                transport_priority: false,
                pid: Pid::new(0x100).unwrap(),
                transport_scrambling_control: TransportScramblingControl::NotScrambled,
                continuity_counter: ContinuityCounter::new(),
            },
            payload: Some(TsPayload::Raw(Bytes::new(&[0x00; 184]).unwrap())),
            adaptation_field: None,
        };
        assert!(decoder.process_ts_packet(&packet).is_ok());
    }

    #[test]
    fn test_pes_packet_decoder_pes_payload() {
        let mut decoder = PesPacketDecoder::new();
        let pes_packet = Pes {
            header: PesHeader {
                stream_id: StreamId::new(0x1),
                priority: false,
                data_alignment_indicator: false,
                copyright: false,
                original_or_copy: true,
                pts: None,
                dts: None,
                escr: None,
            },
            pes_packet_len: 35,
            data: Bytes::new(&[0x00; 32]).unwrap(),
        };
        let packet = TsPacket {
            header: TsHeader {
                transport_error_indicator: false,
                transport_priority: false,
                pid: Pid::new(0x100).unwrap(),
                transport_scrambling_control: TransportScramblingControl::NotScrambled,
                continuity_counter: ContinuityCounter::new(),
            },
            payload: Some(TsPayload::Pes(pes_packet)),
            adaptation_field: None,
        };

        // first packet returned will be None
        let result = decoder.process_ts_packet(&packet);
        assert!(result.is_ok());
        let pes_packet = result.unwrap();
        assert!(pes_packet.is_none());

        let result = decoder.process_ts_packet(&packet);
        assert!(result.is_ok());
        let pes_packet = result.unwrap();
        assert!(pes_packet.is_some());
    }

    #[test]
    fn test_pes_packet_decoder_flush() {
        let mut decoder = PesPacketDecoder::new();
        let pes_packet = Pes {
            header: PesHeader {
                stream_id: StreamId::new(0x1),
                priority: false,
                data_alignment_indicator: false,
                copyright: false,
                original_or_copy: true,
                pts: None,
                dts: None,
                escr: None,
            },
            pes_packet_len: 35,
            data: Bytes::new(&[0x00; 32]).unwrap(),
        };
        let packet = TsPacket {
            header: TsHeader {
                transport_error_indicator: false,
                transport_priority: false,
                pid: Pid::new(0x100).unwrap(),
                transport_scrambling_control: TransportScramblingControl::NotScrambled,
                continuity_counter: ContinuityCounter::new(),
            },
            payload: Some(TsPayload::Pes(pes_packet)),
            adaptation_field: None,
        };

        let result = decoder.process_ts_packet(&packet);
        assert!(result.is_ok());

        let result = decoder.flush();
        assert!(result.is_ok());
        let p = result.unwrap();
        assert!(p.is_some());
    }
}
