extern crate clap;
extern crate mpeg2ts;
#[macro_use]
extern crate trackable;

use clap::{App, Arg};
use mpeg2ts::pes::{PesPacketReader, ReadPesPacket};
use mpeg2ts::ts::{ReadTsPacket, TsHeader, TsPacketReader, TsPacketWriter, WriteTsPacket};
use std::io::Write;
use trackable::error::Failure;

fn main() {
    let matches = App::new("parse")
        .arg(
            Arg::with_name("OUTPUT_TYPE")
                .long("output-type")
                .short("o")
                .takes_value(true)
                .possible_values(&[
                    "ts",
                    "ts-packet",
                    "pes-packet",
                    "es-audio",
                    "es-video",
                    "klv",
                    "header",
                ])
                .default_value("ts-packet"),
        )
        .arg(
            Arg::with_name("VERBOSE")
                .long("verbose")
                .short("v")
                .takes_value(false)
                .default_value(""),
        )
        .get_matches();
    match matches.value_of("OUTPUT_TYPE").unwrap() {
        "ts" => {
            let mut writer = TsPacketWriter::new(std::io::stdout());
            let mut reader = TsPacketReader::new(std::io::stdin());
            while let Some(packet) = track_try_unwrap!(reader.read_ts_packet()) {
                track_try_unwrap!(writer.write_ts_packet(&packet));
            }
        }
        "ts-packet" => {
            let mut seen: Vec<u16> = Vec::new();
            let mut reader = TsPacketReader::new(std::io::stdin());
            while let Some(packet) = track_try_unwrap!(reader.read_ts_packet()) {
                //println!("{:?}", packet);
                let pid = packet.header.pid.as_u16();
                if !seen.contains(&pid) {
                    seen.push(pid);
                    println!("{:?}", pid);
                }
            }
        }
        "pes-packet" => {
            let mut reader = PesPacketReader::new(TsPacketReader::new(std::io::stdin()));
            while let Some(packet) = track_try_unwrap!(reader.read_pes_packet()) {
                println!("{:?} {} bytes", packet.header, packet.data.len());
            }
        }
        "es-audio" => {
            let mut reader = PesPacketReader::new(TsPacketReader::new(std::io::stdin()));
            while let Some(packet) = track_try_unwrap!(reader.read_pes_packet()) {
                if !packet.header.stream_id.is_audio() {
                    continue;
                }
                track_try_unwrap!(std::io::stdout()
                    .write_all(&packet.data)
                    .map_err(Failure::from_error));
            }
        }
        "es-video" => {
            let mut reader = PesPacketReader::new(TsPacketReader::new(std::io::stdin()));
            while let Some(packet) = track_try_unwrap!(reader.read_pes_packet()) {
                if !packet.header.stream_id.is_video() {
                    continue;
                }
                track_try_unwrap!(std::io::stdout()
                    .write_all(&packet.data)
                    .map_err(Failure::from_error));
            }
        }
        "klv" => {
            let mut reader = PesPacketReader::new(TsPacketReader::new(std::io::stdin()));
            let mut seen: Vec<u16> = Vec::new();
            while let Some(packet) = track_try_unwrap!(reader.read_pes_packet()) {
                // track_try_unwrap!(
                //     std::io::stdout()
                //         .write_all(&packet.data)
                //         .map_err(Failure::from_error)
                // );
                if packet.header.stream_id.is_video() {
                    println!("Video PTS: {:?}, Video DTS: {:?}", packet.header.pts, packet.header.dts);
                } else if packet.header.stream_id.is_klv() {
                    /*track_try_unwrap!(std::io::stdout()
                    .write_all(&packet.data)
                    .map_err(Failure::from_error));*/
                    //println!("{:0X?}",&packet.header.stream_id);
                    let pid = packet.header.stream_id.as_u8() as u16;
                    if !seen.contains(&pid) {
                        seen.push(pid);
                        if packet.header.stream_id.is_async_klv() {
                            println!("Async Packet 0x{:0X}", pid);
                        } else {
                            println!("Sync Packet 0x{:0X}", pid);
                        }
                    }

                    if matches.is_present("VERBOSE") {
                        println!("KLV PTS: {:?}, KLV DTS: {:?}", packet.header.pts, packet.header.dts);
                        //println!("{:X?}", packet.data);
                        println!();
                    }
                }
                /*if packet.data.len() > 0 {
                    println!("{:0X?}",&packet.data);
                }*/
            }
        }
        "header" => {
            let mut reader = PesPacketReader::new(TsPacketReader::new(std::io::stdin()));
            let mut seen: Vec<u8> = Vec::new();
            while let Some(packet) = track_try_unwrap!(reader.read_pes_packet()) {
                let id = packet.header.stream_id.as_u8();
                if !seen.contains(&id) {
                    seen.push(id);
                    println!("0x{:0X?}",id);
                }
            }
        }
        _ => unreachable!(),
    }
}
