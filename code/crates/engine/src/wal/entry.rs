use std::io::{self, Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt, BE};

use malachitebft_codec::Codec;
use malachitebft_core_consensus::{ProposedValue, SignedConsensusMsg};
use malachitebft_core_types::{Context, Round, Timeout};

/// Codec for encoding and decoding WAL entries.
///
/// This trait is automatically implemented for any type that implements:
/// - [`Codec<SignedConsensusMsg<Ctx>>`]
pub trait WalCodec<Ctx>
where
    Ctx: Context,
    Self: Codec<Ctx::Height>,
    Self: Codec<SignedConsensusMsg<Ctx>>,
    Self: Codec<ProposedValue<Ctx>>,
{
}

impl<Ctx, C> WalCodec<Ctx> for C
where
    Ctx: Context,
    C: Codec<Ctx::Height>,
    C: Codec<SignedConsensusMsg<Ctx>>,
    C: Codec<ProposedValue<Ctx>>,
{
}

pub use malachitebft_core_consensus::WalEntry;

const TAG_CONSENSUS: u8 = 0x01;
const TAG_TIMEOUT: u8 = 0x02;
const TAG_PROPOSED_VALUE: u8 = 0x04;

pub fn encode_entry<Ctx, C, W>(entry: &WalEntry<Ctx>, codec: &C, buf: W) -> io::Result<()>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    W: Write,
{
    match entry {
        WalEntry::ConsensusMsg(msg) => encode_consensus_msg(TAG_CONSENSUS, msg, codec, buf),
        WalEntry::Timeout(timeout) => encode_timeout(TAG_TIMEOUT, timeout, codec, buf),
        WalEntry::ProposedValue(value) => {
            encode_proposed_value(TAG_PROPOSED_VALUE, value, codec, buf)
        }
    }
}

pub fn decode_entry<Ctx, C, R>(codec: &C, mut buf: R) -> io::Result<WalEntry<Ctx>>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    R: Read,
{
    let tag = buf.read_u8()?;

    match tag {
        TAG_CONSENSUS => decode_consensus_msg(codec, buf).map(WalEntry::ConsensusMsg),
        TAG_TIMEOUT => decode_timeout(codec, buf).map(WalEntry::Timeout),
        TAG_PROPOSED_VALUE => decode_proposed_value(codec, buf).map(WalEntry::ProposedValue),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "invalid tag")),
    }
}

// Consensus message helpers
fn encode_consensus_msg<Ctx, C, W>(
    tag: u8,
    msg: &SignedConsensusMsg<Ctx>,
    codec: &C,
    mut buf: W,
) -> io::Result<()>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    W: Write,
{
    let bytes = codec.encode(msg).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to encode consensus message: {e}"),
        )
    })?;

    // Write tag
    buf.write_u8(tag)?;

    // Write encoded length
    buf.write_u64::<BE>(bytes.len() as u64)?;

    // Write encoded bytes
    buf.write_all(&bytes)?;

    Ok(())
}

fn decode_consensus_msg<Ctx, C, R>(codec: &C, mut buf: R) -> io::Result<SignedConsensusMsg<Ctx>>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    R: Read,
{
    let len = buf.read_u64::<BE>()?;
    let mut bytes = vec![0; len as usize];
    buf.read_exact(&mut bytes)?;

    codec.decode(bytes.into()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to decode consensus msg: {e}"),
        )
    })
}

// Timeout helpers
fn encode_timeout<Ctx, C>(
    tag: u8,
    timeout: &Timeout<Ctx>,
    codec: &C,
    mut buf: impl Write,
) -> io::Result<()>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
{
    use malachitebft_core_types::TimeoutKind;

    let step = match timeout.kind {
        TimeoutKind::Propose => 1,
        TimeoutKind::Prevote => 2,
        TimeoutKind::Precommit => 3,

        // NOTE: Commit, prevote and precommit time limit timeouts have been removed.

        // Consensus will typically not want to store these timeouts in the WAL,
        // but we still need to handle them here.
        TimeoutKind::Rebroadcast => 7,
    };

    buf.write_u8(tag)?;
    buf.write_u8(step)?;
    buf.write_i64::<BE>(timeout.round.as_i64())?;

    // Encode the height using the codec
    let height_bytes = codec.encode(&timeout.height).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to encode timeout height: {e}"),
        )
    })?;

    buf.write_u64::<BE>(height_bytes.len() as u64)?;
    buf.write_all(&height_bytes)?;

    Ok(())
}

fn decode_timeout<Ctx, C>(codec: &C, mut buf: impl Read) -> io::Result<Timeout<Ctx>>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
{
    use malachitebft_core_types::TimeoutKind;

    let step = match buf.read_u8()? {
        1 => TimeoutKind::Propose,
        2 => TimeoutKind::Prevote,
        3 => TimeoutKind::Precommit,

        // Commit timeouts have been removed in PR #976,
        // but we still need to handle them here in order to decode old WAL entries.
        4 => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "commit timeouts are no longer supported, ignoring",
            ))
        }

        // Prevote/precommit rebroadcast timeouts have been removed in PR #1037,
        // but we still need to handle them here in order to decode old WAL entries.
        5 | 6 => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "prevote/precommit time limit timeouts are no longer supported, ignoring",
            ))
        }

        // Consensus will typically not want to store these timeouts in the WAL,
        // but we still need to handle them here.
        7 => TimeoutKind::Rebroadcast,

        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid timeout step",
            ))
        }
    };

    let round = Round::from(buf.read_i64::<BE>()?);

    // Decode the height using the codec
    let len = buf.read_u64::<BE>()?;
    let mut height_bytes = vec![0; len as usize];
    buf.read_exact(&mut height_bytes)?;

    let height = codec.decode(height_bytes.into()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to decode timeout height: {e}"),
        )
    })?;

    Ok(Timeout::new(height, round, step))
}

// Proposed value helpers
fn encode_proposed_value<Ctx, C, W>(
    tag: u8,
    value: &ProposedValue<Ctx>,
    codec: &C,
    mut buf: W,
) -> io::Result<()>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    W: Write,
{
    let bytes = codec.encode(value).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to encode consensus message: {e}"),
        )
    })?;

    // Write tag
    buf.write_u8(tag)?;

    // Write encoded length
    buf.write_u64::<BE>(bytes.len() as u64)?;

    // Write encoded bytes
    buf.write_all(&bytes)?;

    Ok(())
}

fn decode_proposed_value<Ctx, C, R>(codec: &C, mut buf: R) -> io::Result<ProposedValue<Ctx>>
where
    Ctx: Context,
    C: WalCodec<Ctx>,
    R: Read,
{
    let len = buf.read_u64::<BE>()?;
    let mut bytes = vec![0; len as usize];
    buf.read_exact(&mut bytes)?;

    codec.decode(bytes.into()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to decode proposed value: {e}"),
        )
    })
}
