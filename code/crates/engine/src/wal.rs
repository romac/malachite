use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use derive_where::derive_where;
use eyre::eyre;
use ractor::{async_trait, Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SpawnErr};
use tracing::{debug, error, info};
use tracing::{warn, Span};

use malachitebft_core_types::{Context, Height};
use malachitebft_metrics::SharedRegistry;
use malachitebft_wal as wal;

mod entry;
mod iter;

pub use entry::WalCodec;
pub use entry::WalEntry;
pub use iter::log_entries;

use crate::wal::entry::{decode_entry, encode_entry};

pub type WalRef<Ctx> = ActorRef<Msg<Ctx>>;

pub struct Wal<Ctx, Codec> {
    span: tracing::Span,
    _marker: PhantomData<(Ctx, Codec)>,
}

impl<Ctx, Codec> Wal<Ctx, Codec>
where
    Ctx: Context,
    Codec: WalCodec<Ctx>,
{
    pub fn new(span: tracing::Span) -> Self {
        Self {
            span,
            _marker: PhantomData,
        }
    }

    pub async fn spawn(
        _ctx: &Ctx,
        codec: Codec,
        path: PathBuf,
        _metrics: SharedRegistry,
        span: tracing::Span,
    ) -> Result<WalRef<Ctx>, SpawnErr> {
        let (actor_ref, _) = Actor::spawn(None, Self::new(span), Args { path, codec }).await?;
        Ok(actor_ref)
    }
}

pub type WalReply<T> = RpcReplyPort<eyre::Result<T>>;

pub enum Msg<Ctx: Context> {
    StartedHeight(Ctx::Height, WalReply<Option<Vec<WalEntry<Ctx>>>>),
    Reset(Ctx::Height, WalReply<()>),
    Append(Ctx::Height, WalEntry<Ctx>, WalReply<()>),
    Flush(WalReply<()>),
    Dump,
}

pub struct Args<Codec> {
    pub path: PathBuf,
    pub codec: Codec,
}

#[derive_where(Clone)]
pub struct State<Ctx: Context, Codec> {
    height: Ctx::Height,
    log: Arc<Mutex<wal::Log>>,
    codec: Arc<Codec>,
}

impl<Ctx, Codec> Wal<Ctx, Codec>
where
    Ctx: Context,
    Codec: WalCodec<Ctx>,
{
    async fn handle_msg(
        &self,
        _myself: WalRef<Ctx>,
        msg: Msg<Ctx>,
        state: &mut State<Ctx, Codec>,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            Msg::StartedHeight(height, reply_to) => {
                if state.height == height {
                    debug!(%height, "WAL already at height, ignoring");
                    return Ok(());
                }

                state.height = height;

                tokio::task::spawn_blocking({
                    let span = Span::current();
                    let state = state.clone();
                    move || started_height(span, state, height, reply_to)
                })
                .await??;
            }

            Msg::Reset(height, reply_to) => {
                tokio::task::spawn_blocking({
                    let span = Span::current();
                    let state = state.clone();
                    move || reset(span, state, height, reply_to)
                })
                .await??;
            }

            Msg::Append(height, entry, reply_to) => {
                if height != state.height {
                    warn!(wal.height = %state.height, entry.height = %height, "Ignoring append, mismatched height");
                    return Ok(());
                }

                tokio::task::spawn_blocking({
                    let span = Span::current();
                    let state = state.clone();
                    move || append(span, state, entry, reply_to)
                })
                .await??;
            }

            Msg::Flush(reply_to) => {
                tokio::task::spawn_blocking({
                    let span = Span::current();
                    let state = state.clone();
                    move || flush(span, state, reply_to)
                })
                .await??;
            }

            Msg::Dump => {
                tokio::task::spawn_blocking({
                    let span = Span::current();
                    let state = state.clone();
                    move || dump(span, state)
                })
                .await??;
            }
        }

        Ok(())
    }
}

#[tracing::instrument(parent = &span, skip_all)]
fn started_height<Ctx: Context, Codec: WalCodec<Ctx>>(
    span: tracing::Span,
    state: State<Ctx, Codec>,
    height: <Ctx as Context>::Height,
    reply_to: WalReply<Option<Vec<WalEntry<Ctx>>>>,
) -> Result<(), ActorProcessingErr> {
    let sequence = height.as_u64();

    let mut log = state
        .log
        .lock()
        .map_err(|e| eyre!("Failed to lock WAL for flushing: {e}"))?;

    let entries = if sequence == log.sequence() {
        // WAL is already at that sequence
        // Let's check if there are any entries to replay
        fetch_entries(&mut log, state.codec.as_ref()).map(Some)
    } else {
        // WAL is at different sequence, restart it
        // No entries to replay
        let result = log.restart(sequence).map(|_| None).map_err(Into::into);

        debug!(%height, "Reset WAL");

        result
    };

    drop(log);

    reply_to
        .send(entries)
        .map_err(|e| eyre!("Failed to send reply: {e}"))?;

    Ok(())
}

#[tracing::instrument(parent = &span, skip_all)]
fn reset<Ctx: Context, Codec: WalCodec<Ctx>>(
    span: tracing::Span,
    state: State<Ctx, Codec>,
    height: Ctx::Height,
    reply_to: WalReply<()>,
) -> Result<(), ActorProcessingErr> {
    let sequence = height.as_u64();

    let mut log = state
        .log
        .lock()
        .map_err(|e| eyre!("Failed to lock WAL for flushing: {e}"))?;

    let result = log.restart(sequence).map_err(Into::into);

    drop(log);

    debug!(%height, "Reset WAL");

    reply_to
        .send(result)
        .map_err(|e| eyre!("Failed to send reply: {e}"))?;

    Ok(())
}

#[tracing::instrument(parent = &span, skip_all)]
fn append<Ctx: Context, Codec: WalCodec<Ctx>>(
    span: tracing::Span,
    state: State<Ctx, Codec>,
    entry: WalEntry<Ctx>,
    reply_to: WalReply<()>,
) -> Result<(), ActorProcessingErr> {
    let entry_type = wal_entry_type(&entry);

    let mut buf = Vec::new();
    encode_entry(&entry, state.codec.as_ref(), &mut buf)?;

    if !buf.is_empty() {
        let mut log = state
            .log
            .lock()
            .map_err(|e| eyre!("Failed to lock WAL for flushing: {e}"))?;

        let result = log.append(&buf).map_err(Into::into);

        if let Err(e) = &result {
            error!("ATTENTION: Failed to append entry to WAL: {e}");
        } else {
            debug!(
                type = %entry_type, entry.size = %buf.len(), log.entries = %log.len(),
                "Wrote log entry"
            );
        }

        drop(log);

        reply_to
            .send(result)
            .map_err(|e| eyre!("Failed to send reply: {e}"))?;
    }

    Ok(())
}

#[tracing::instrument(parent = &span, skip_all)]
fn flush<Ctx: Context, Codec: WalCodec<Ctx>>(
    span: tracing::Span,
    state: State<Ctx, Codec>,
    reply_to: WalReply<()>,
) -> Result<(), ActorProcessingErr> {
    let mut log = state
        .log
        .lock()
        .map_err(|e| eyre!("Failed to lock WAL for flushing: {e}"))?;

    let result = log.flush().map_err(Into::into);

    if let Err(e) = &result {
        error!("ATTENTION: Failed to flush WAL to disk: {e}");
    } else {
        debug!(
            wal.entries = %log.len(),
            wal.size = %log.size_bytes().unwrap_or(0),
            "Flushed WAL to disk"
        );
    }

    drop(log);

    reply_to
        .send(result)
        .map_err(|e| eyre!("Failed to send reply: {e}"))?;

    Ok(())
}

#[tracing::instrument(parent = &span, skip_all)]
fn dump<Ctx: Context, Codec: WalCodec<Ctx>>(
    span: tracing::Span,
    state: State<Ctx, Codec>,
) -> Result<(), ActorProcessingErr> {
    let mut log = state
        .log
        .lock()
        .map_err(|e| eyre!("Failed to lock WAL for flushing: {e}"))?;

    dump_entries(&mut log, state.codec.as_ref())
        .map_err(|e| eyre!("Failed to dump WAL entries: {e}"))?;

    Ok(())
}

#[async_trait]
impl<Ctx, Codec> Actor for Wal<Ctx, Codec>
where
    Ctx: Context,
    Codec: WalCodec<Ctx>,
{
    type Msg = Msg<Ctx>;
    type Arguments = Args<Codec>;
    type State = State<Ctx, Codec>;

    #[tracing::instrument(
        name = "wal.pre_start",
        parent = &self.span,
        skip_all,
    )]
    async fn pre_start(
        &self,
        _myself: WalRef<Ctx>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let path = args.path.clone();

        info!("Opening WAL at {}", args.path.display());
        let log = tokio::task::spawn_blocking(move || wal::Log::open(&path)).await??;
        info!("Opened WAL at {}", args.path.display());

        Ok(State {
            height: Ctx::Height::ZERO,
            log: Arc::new(Mutex::new(log)),
            codec: Arc::new(args.codec),
        })
    }

    #[tracing::instrument(
        name = "wal",
        parent = &self.span,
        skip_all,
        fields(height = %span_height(state.height, &msg)),
    )]
    async fn handle(
        &self,
        myself: WalRef<Ctx>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Err(e) = self.handle_msg(myself, msg, state).await {
            error!("Failed to handle WAL message: {e}");
        }

        Ok(())
    }

    #[tracing::instrument(
        name = "wal.post_stop",
        parent = &self.span,
        skip_all,
        fields(height = %state.height),
    )]
    async fn post_stop(
        &self,
        _: WalRef<Ctx>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        info!("Shutting down WAL");

        Ok(())
    }
}

/// Use the height we are about to start instead of the current height
/// for the tracing span of the WAL actor when starting a new height.
fn span_height<Ctx: Context>(height: Ctx::Height, msg: &Msg<Ctx>) -> Ctx::Height {
    if let Msg::StartedHeight(h, _) = msg {
        *h
    } else {
        height
    }
}

fn fetch_entries<Ctx, Codec>(log: &mut wal::Log, codec: &Codec) -> eyre::Result<Vec<WalEntry<Ctx>>>
where
    Ctx: Context,
    Codec: WalCodec<Ctx>,
{
    if log.is_empty() {
        return Ok(Vec::new());
    }

    let entries = log
        .iter()?
        .enumerate() // Add enumeration to get the index
        .filter_map(|(idx, result)| match result {
            Ok(entry) => Some((idx, entry)),
            Err(e) => {
                error!("Failed to retrieve WAL entry {idx}: {e}");
                None
            }
        })
        .filter_map(
            |(idx, bytes)| match decode_entry(codec, io::Cursor::new(bytes.clone())) {
                Ok(entry) => Some(entry),
                Err(e) => {
                    error!("Failed to decode WAL entry {idx}: {e} {:?}", bytes);
                    None
                }
            },
        )
        .collect::<Vec<_>>();

    if log.len() != entries.len() {
        Err(eyre::eyre!(
            "Failed to fetch and decode all WAL entries: expected {}, got {}",
            log.len(),
            entries.len()
        ))
    } else {
        Ok(entries)
    }
}

fn dump_entries<'a, Ctx, Codec>(log: &'a mut wal::Log, codec: &'a Codec) -> eyre::Result<()>
where
    Ctx: Context,
    Codec: WalCodec<Ctx>,
{
    let len = log.len();
    let mut count = 0;

    info!("WAL Dump");
    info!("- Entries: {len}");
    info!("- Size:    {} bytes", log.size_bytes().unwrap_or(0));
    info!("Entries:");

    for (idx, entry) in log_entries(log, codec)?.enumerate() {
        count += 1;

        match entry {
            Ok(entry) => {
                info!("- #{idx}: {entry:?}");
            }
            Err(e) => {
                error!("- #{idx}: Error decoding WAL entry: {e}");
            }
        }
    }

    if count != len {
        error!("Expected {len} entries, but found {count} entries");
    }

    Ok(())
}

fn wal_entry_type<Ctx: Context>(entry: &WalEntry<Ctx>) -> &'static str {
    use malachitebft_core_consensus::SignedConsensusMsg;

    match entry {
        WalEntry::ConsensusMsg(msg) => match msg {
            SignedConsensusMsg::Vote(_) => "Consensus(Vote)",
            SignedConsensusMsg::Proposal(_) => "Consensus(Proposal)",
        },
        WalEntry::ProposedValue(_) => "LocallyProposedValue",
        WalEntry::Timeout(_) => "Timeout",
    }
}
