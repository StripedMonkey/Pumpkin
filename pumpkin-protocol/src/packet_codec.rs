use std::io::{Read, Write as _};

use crate::{
    raw_packet::{CompressedPacket, RawPacket, UncompressedPacket},
    PacketError, VarInt, VarIntDecodeError,
};
use bytes::{Buf, BufMut};
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use tokio_util::codec::{Decoder, Encoder};

#[derive(Default)]
pub struct RawPacketCodec {
    current_packet_len: Option<usize>,
}

impl Decoder for RawPacketCodec {
    type Item = RawPacket;

    type Error = PacketError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let packet_len = match self.current_packet_len {
            Some(len) => len,
            None => match VarInt::decode_partial_buf(src) {
                Ok(len) => {
                    // TODO: this is actually somewhat unsafe, since we allocate assuming the client is honest about packet size. This should be limited
                    src.reserve(len as usize - src.len());
                    self.current_packet_len = Some(len as usize);
                    len as usize
                }
                Err(VarIntDecodeError::Incomplete) => return Ok(None),
                Err(VarIntDecodeError::TooLarge) => return Err(PacketError::MalformedLength)?,
            },
        };
        if src.len() < packet_len {
            return Ok(None);
        }
        let packet_data = src.split_off(packet_len);
        Ok(Some(RawPacket::new(packet_data)))
    }
}

impl Encoder<RawPacket> for RawPacketCodec {
    type Error = PacketError;

    fn encode(&mut self, item: RawPacket, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        // A raw packet is always prefixed by a VarInt indicating its length, then the data.
        VarInt::from(item.len() as i32).encode(dst.writer())?;
        dst.extend(item.into_inner());
        Ok(())
    }
}

#[derive(Default)]
pub struct UncompressedPacketCodec {
    raw_codec: RawPacketCodec,
}

impl Decoder for UncompressedPacketCodec {
    type Item = UncompressedPacket;

    type Error = PacketError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Decode the raw packet frame
        let raw_packet = match self.raw_codec.decode(src)? {
            Some(packet) => packet.into_inner(),
            None => return Ok(None),
        };
        // Packet ID comes first
        Ok(Some(UncompressedPacket::try_from(raw_packet)?))
    }
}

impl Encoder<UncompressedPacket> for UncompressedPacketCodec {
    type Error = PacketError;

    fn encode(
        &mut self,
        item: UncompressedPacket,
        dst: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        // Encode the packet length
        VarInt::from(item.len()).encode(dst.writer())?;
        // Encode the packet ID
        VarInt::from(item.packet_id()).encode(dst.writer())?;
        // Encode the packet data
        dst.extend(item.into_inner());
        Ok(())
    }
}

struct CompressedPacketCodec {
    raw_codec: RawPacketCodec,
    compression_level: flate2::Compression,
    threshold: Option<usize>,
}

impl CompressedPacketCodec {
    pub fn new(compression_level: flate2::Compression, threshold: Option<usize>) -> Self {
        Self {
            raw_codec: RawPacketCodec::default(),
            compression_level,
            threshold,
        }
    }

    /// Compress an uncompressed packet
    fn compress(&self, packet: UncompressedPacket) -> Result<CompressedPacket, PacketError> {
        if let Some(threshold) = self.threshold {
            if packet.len() < threshold {
                let mut bytes = bytes::BytesMut::new();
                UncompressedPacketCodec::default().decode(&mut bytes)?;
                return Ok(CompressedPacket::new(0, bytes));
            }
        }
        let uncompressed_size = packet.len();
        let compressed_bytes = bytes::BytesMut::new();
        let mut encoder = ZlibEncoder::new(compressed_bytes.writer(), self.compression_level);
        VarInt::from(packet.packet_id()).encode(&mut encoder)?;
        encoder.write_all(&packet.into_inner())?;
        let compressed_bytes = encoder.finish()?.into_inner();
        Ok(CompressedPacket::new(uncompressed_size, compressed_bytes))
    }

    fn decompress(&self, packet: CompressedPacket) -> Result<UncompressedPacket, PacketError> {
        let decompressed_length = packet.len();
        let mut decoder = ZlibDecoder::new(packet.into_inner().reader());
        let mut decompressed = bytes::BytesMut::zeroed(decompressed_length);
        decoder.read_exact(&mut decompressed)?;
        Ok(UncompressedPacket::try_from(decompressed)?)
    }
}

impl Decoder for CompressedPacketCodec {
    type Item = CompressedPacket;

    type Error = PacketError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let packet_data = match self.raw_codec.decode(src)? {
            Some(packet) => packet.into_inner(),
            None => return Ok(None),
        };
        Ok(Some(CompressedPacket::try_from(packet_data)?))
    }
}

impl Encoder<UncompressedPacket> for CompressedPacketCodec {
    type Error = PacketError;

    fn encode(
        &mut self,
        item: UncompressedPacket,
        dst: &mut bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let compressed = self.compress(item)?;
        VarInt::from(compressed.len()).encode(dst.writer())?;
        dst.extend(compressed.into_inner());
        Ok(())
    }
}
