use std::collections::VecDeque;

use pes::PesPacket;
use ts::ReadTsPacket;

use super::PesPacketDecoder;

use {ErrorKind, Result};

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

    /// Returns true if the reader has a back buffer.
    fn has_back_buffer(&self) -> bool;
}

/// PES packet reader.
#[derive(Debug)]
pub struct PesPacketReader<R> {
    peeked_packet: Option<PesPacket<Vec<u8>>>,
    ts_packet_reader: R,
    pes_decoder: PesPacketDecoder,
    eos: bool,
    is_marked: bool,
    back_buffer: VecDeque<PesPacket<Vec<u8>>>,
}
impl<R: ReadTsPacket> PesPacketReader<R> {
    /// Makes a new `PesPacketReader` instance.
    pub fn new(ts_packet_reader: R) -> Self {
        PesPacketReader {
            peeked_packet: None,
            ts_packet_reader,
            pes_decoder: PesPacketDecoder::new(),
            eos: false,
            is_marked: false,
            back_buffer: VecDeque::<PesPacket<Vec<u8>>>::with_capacity(200),
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

    fn read_next_pes_packet(&mut self) -> Result<Option<PesPacket<Vec<u8>>>> {
        if self.eos {
            return track!(self.pes_decoder.flush());
        }

        while let Some(ts_packet) = track!(self.ts_packet_reader.read_ts_packet())? {
            if let Ok(result) = self.pes_decoder.process_ts_packet(&ts_packet) {
                return Ok(result);
            }
        }

        self.eos = true;
        track!(self.pes_decoder.flush())
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
