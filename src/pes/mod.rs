//! Packetized elementary stream.
//!
//! # References
//!
//! - [Packetized elementary stream](https://en.wikipedia.org/wiki/Packetized_elementary_stream)
pub use self::decoder::PesPacketDecoder;
pub use self::packet::{PesHeader, PesPacket};
pub use self::reader::{PesPacketReader, ReadPesPacket};

mod decoder;
mod packet;
mod reader;

#[derive(Debug)]
struct PartialPesPacket {
    packet: PesPacket<Vec<u8>>,
    data_len: Option<usize>,
}
