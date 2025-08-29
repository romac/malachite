//! Run Malachite consensus with the given configuration and context.
//! Provides the application with a channel for receiving messages from consensus.

use std::path::Path;

use eyre::Result;
use ractor::async_trait;
use tokio::task::JoinHandle;

use malachitebft_config::{ConsensusConfig, ValueSyncConfig};
use malachitebft_engine::consensus::ConsensusRef;
use malachitebft_engine::host::HostRef;
use malachitebft_engine::network::NetworkRef;
use malachitebft_engine::node::NodeRef;
use malachitebft_engine::sync::SyncRef;
use malachitebft_engine::util::events::TxEvent;
use malachitebft_engine::wal::WalRef;

use crate::metrics::{Metrics, SharedRegistry};
use crate::node::{self, EngineHandle, NodeConfig};
use crate::spawn::{
    spawn_consensus_actor, spawn_network_actor, spawn_node_actor, spawn_sync_actor, spawn_wal_actor,
};
use crate::types::core::{Context, SigningProvider};
use crate::types::{codec, Keypair};

pub struct NetworkDeps<'a, NetCodec> {
    pub cfg: &'a ConsensusConfig,
    pub keypair: Keypair,
    pub registry: &'a SharedRegistry,
    pub codec: NetCodec,
}

pub struct WalDeps<'a, Ctx: Context, WalCodec> {
    pub ctx: &'a Ctx,
    pub wal_codec: WalCodec,
    pub home_dir: &'a Path,
    pub registry: &'a SharedRegistry,
}

pub struct HostDeps {
    pub metrics: Metrics,
}

pub struct SyncDeps<'a, Ctx: Context> {
    pub ctx: Ctx,
    pub network: NetworkRef<Ctx>,
    pub host: HostRef<Ctx>,
    pub cfg: &'a ValueSyncConfig,
    pub registry: &'a SharedRegistry,
}

pub struct ConsensusDeps<'a, Ctx: Context> {
    pub ctx: Ctx,
    pub start_height: Ctx::Height,
    pub initial_validator_set: Ctx::ValidatorSet,
    pub address: Ctx::Address,
    pub consensus_cfg: ConsensusConfig,
    pub sync_cfg: &'a ValueSyncConfig,
    pub signing_provider: Box<dyn SigningProvider<Ctx>>,
    pub network: NetworkRef<Ctx>,
    pub host: HostRef<Ctx>,
    pub wal: WalRef<Ctx>,
    pub sync: Option<SyncRef<Ctx>>,
    pub metrics: Metrics,
    pub tx_event: TxEvent<Ctx>,
}

pub struct NodeDeps<Ctx: Context> {
    pub ctx: Ctx,
    pub network: NetworkRef<Ctx>,
    pub consensus: ConsensusRef<Ctx>,
    pub wal: WalRef<Ctx>,
    pub sync: Option<SyncRef<Ctx>>,
    pub host: HostRef<Ctx>,
}

#[async_trait]
pub trait NetworkSpawner<Ctx, NetCodec>: Send + Sync
where
    Ctx: Context,
    NetCodec: Clone + Send + Sync,
{
    async fn spawn(&self, deps: NetworkDeps<'_, NetCodec>) -> Result<NetworkRef<Ctx>>;
}

#[async_trait]
pub trait WalSpawner<Ctx, WalCodec>: Send + Sync
where
    Ctx: Context,
    WalCodec: Clone + Send + Sync + 'static,
{
    async fn spawn(&self, deps: WalDeps<'_, Ctx, WalCodec>) -> Result<WalRef<Ctx>>;
}

#[async_trait]
pub trait HostSpawner<Ctx: Context>: Send + Sync {
    async fn spawn(&self, deps: HostDeps) -> Result<HostRef<Ctx>>;
}

#[async_trait]
pub trait SyncSpawner<Ctx: Context>: Send + Sync {
    async fn spawn(&self, deps: SyncDeps<'_, Ctx>) -> Result<Option<SyncRef<Ctx>>>;
}

#[async_trait]
pub trait ConsensusSpawner<Ctx: Context>: Send + Sync {
    async fn spawn(&self, deps: ConsensusDeps<'_, Ctx>) -> Result<ConsensusRef<Ctx>>;
}

#[async_trait]
pub trait NodeSpawner<Ctx: Context>: Send + Sync {
    async fn spawn(&self, deps: NodeDeps<Ctx>) -> Result<(NodeRef, JoinHandle<()>)>;
}

pub struct DefaultNetworkSpawner;

#[async_trait]
impl<Ctx, NetCodec> NetworkSpawner<Ctx, NetCodec> for DefaultNetworkSpawner
where
    Ctx: Context,
    NetCodec: codec::ConsensusCodec<Ctx> + codec::SyncCodec<Ctx> + Clone,
{
    async fn spawn(&self, deps: NetworkDeps<'_, NetCodec>) -> Result<NetworkRef<Ctx>> {
        spawn_network_actor(deps.cfg, deps.keypair, deps.registry, deps.codec).await
    }
}

pub struct DefaultWalSpawner;

#[async_trait]
impl<Ctx, WalCodec> WalSpawner<Ctx, WalCodec> for DefaultWalSpawner
where
    Ctx: Context,
    WalCodec: codec::WalCodec<Ctx> + Clone,
{
    async fn spawn(&self, deps: WalDeps<'_, Ctx, WalCodec>) -> Result<WalRef<Ctx>> {
        spawn_wal_actor(deps.ctx, deps.wal_codec, deps.home_dir, deps.registry).await
    }
}

pub struct DefaultSyncSpawner;

#[async_trait]
impl<Ctx: Context> SyncSpawner<Ctx> for DefaultSyncSpawner {
    async fn spawn(&self, deps: SyncDeps<'_, Ctx>) -> Result<Option<SyncRef<Ctx>>> {
        spawn_sync_actor(deps.ctx, deps.network, deps.host, deps.cfg, deps.registry).await
    }
}

pub struct DefaultConsensusSpawner;

#[async_trait]
impl<Ctx: Context> ConsensusSpawner<Ctx> for DefaultConsensusSpawner {
    async fn spawn(&self, deps: ConsensusDeps<'_, Ctx>) -> Result<ConsensusRef<Ctx>> {
        spawn_consensus_actor(
            deps.start_height,
            deps.initial_validator_set,
            deps.address,
            deps.ctx,
            deps.consensus_cfg,
            deps.sync_cfg,
            deps.signing_provider,
            deps.network,
            deps.host,
            deps.wal,
            deps.sync,
            deps.metrics,
            deps.tx_event,
        )
        .await
    }
}

pub struct DefaultNodeSpawner;

#[async_trait]
impl<Ctx: Context> NodeSpawner<Ctx> for DefaultNodeSpawner {
    async fn spawn(&self, deps: NodeDeps<Ctx>) -> Result<(NodeRef, JoinHandle<()>)> {
        spawn_node_actor(
            deps.ctx,
            deps.network,
            deps.consensus,
            deps.wal,
            deps.sync,
            deps.host,
        )
        .await
    }
}

pub struct EngineBuilder<Node, Ctx, WalCodec, NetCodec>
where
    Ctx: Context,
    Node: node::Node<Context = Ctx>,
    WalCodec: codec::WalCodec<Ctx> + Clone,
    NetCodec: codec::ConsensusCodec<Ctx> + codec::SyncCodec<Ctx> + Clone,
{
    ctx: Ctx,
    node: Node,
    cfg: Node::Config,
    wal_codec: WalCodec,
    net_codec: NetCodec,
    start_height: Option<Ctx::Height>,
    initial_validator_set: Ctx::ValidatorSet,

    network: Box<dyn NetworkSpawner<Ctx, NetCodec>>,
    wal: Box<dyn WalSpawner<Ctx, WalCodec>>,
    sync: Box<dyn SyncSpawner<Ctx>>,
    consensus: Box<dyn ConsensusSpawner<Ctx>>,
    node_spawner: Box<dyn NodeSpawner<Ctx>>,
}

impl<Node, Ctx, WalCodec, NetCodec> EngineBuilder<Node, Ctx, WalCodec, NetCodec>
where
    Ctx: Context,
    Node: node::Node<Context = Ctx>,
    WalCodec: codec::WalCodec<Ctx> + Clone,
    NetCodec: codec::ConsensusCodec<Ctx> + codec::SyncCodec<Ctx> + Clone,
{
    /// Creates a new `EngineBuilder` with the required configuration and default actors.
    pub fn new(
        ctx: Ctx,
        node: Node,
        cfg: Node::Config,
        wal_codec: WalCodec,
        net_codec: NetCodec,
        start_height: Option<Ctx::Height>,
        initial_validator_set: Ctx::ValidatorSet,
    ) -> Self {
        Self {
            ctx,
            node,
            cfg,
            wal_codec,
            net_codec,
            start_height,
            initial_validator_set,

            // Initialize with default spawner implementations.
            network: Box::new(DefaultNetworkSpawner),
            wal: Box::new(DefaultWalSpawner),
            sync: Box::new(DefaultSyncSpawner),
            consensus: Box::new(DefaultConsensusSpawner),
            node_spawner: Box::new(DefaultNodeSpawner),
        }
    }

    /// Replaces the default network actor spawner with a custom implementation.
    pub fn network<S>(mut self, spawner: S) -> Self
    where
        S: NetworkSpawner<Ctx, NetCodec> + 'static,
    {
        self.network = Box::new(spawner);
        self
    }

    /// Replaces the default WAL actor spawner with a custom implementation.
    pub fn wal<S>(mut self, spawner: S) -> Self
    where
        S: WalSpawner<Ctx, WalCodec> + 'static,
    {
        self.wal = Box::new(spawner);
        self
    }

    /// Replaces the default sync actor spawner with a custom implementation.
    pub fn sync<S>(mut self, spawner: S) -> Self
    where
        S: SyncSpawner<Ctx> + 'static,
    {
        self.sync = Box::new(spawner);
        self
    }

    /// Replaces the default consensus actor spawner with a custom implementation.
    pub fn consensus<S>(mut self, spawner: S) -> Self
    where
        S: ConsensusSpawner<Ctx> + 'static,
    {
        self.consensus = Box::new(spawner);
        self
    }

    /// Replaces the default node actor spawner with a custom implementation.
    pub fn node<S>(mut self, spawner: S) -> Self
    where
        S: NodeSpawner<Ctx> + 'static,
    {
        self.node_spawner = Box::new(spawner);
        self
    }

    /// Set host actor spawner.
    pub fn host<S>(self, spawner: S) -> EngineBuilderWithHost<Node, Ctx, WalCodec, NetCodec>
    where
        S: HostSpawner<Ctx> + 'static,
    {
        EngineBuilderWithHost {
            builder: self,
            host: Box::new(spawner),
        }
    }

    /// Constructs the engine by spawning and connecting all actors using the configured spawners.
    async fn build(self, host: Box<dyn HostSpawner<Ctx>>) -> Result<EngineHandle> {
        let start_height = self.start_height.unwrap_or_default();

        // Initial setup
        let registry = SharedRegistry::global().with_moniker(self.cfg.moniker());
        let metrics = Metrics::register(&registry);

        let private_key_file = self.node.load_private_key_file()?;
        let private_key = self.node.load_private_key(private_key_file);
        let public_key = self.node.get_public_key(&private_key);
        let address = self.node.get_address(&public_key);
        let keypair = self.node.get_keypair(private_key.clone());
        let signing_provider = self.node.get_signing_provider(private_key);

        // Spawn actors sequentially, passing references as needed.

        // Network
        let network_deps = NetworkDeps {
            cfg: self.cfg.consensus(),
            keypair,
            registry: &registry,
            codec: self.net_codec,
        };

        let network = self.network.spawn(network_deps).await?;

        // WAL
        let wal_deps = WalDeps {
            ctx: &self.ctx,
            wal_codec: self.wal_codec,
            home_dir: &self.node.get_home_dir(),
            registry: &registry,
        };

        let wal = self.wal.spawn(wal_deps).await?;

        // Host
        let host_deps = HostDeps {
            metrics: metrics.clone(),
        };

        let host = host.spawn(host_deps).await?;

        // Sync
        let sync_deps = SyncDeps {
            ctx: self.ctx.clone(),
            network: network.clone(),
            host: host.clone(),
            cfg: self.cfg.value_sync(),
            registry: &registry,
        };

        let sync = self.sync.spawn(sync_deps).await?;

        // Consensus
        let tx_event = TxEvent::new();

        let consensus_deps = ConsensusDeps {
            start_height,
            initial_validator_set: self.initial_validator_set,
            address,
            ctx: self.ctx.clone(),
            consensus_cfg: self.cfg.consensus().clone(),
            sync_cfg: self.cfg.value_sync(),
            signing_provider: Box::new(signing_provider),
            network: network.clone(),
            host: host.clone(),
            wal: wal.clone(),
            sync: sync.clone(),
            metrics,
            tx_event: tx_event.clone(),
        };

        let consensus = self.consensus.spawn(consensus_deps).await?;

        // Node
        let node_deps = NodeDeps {
            ctx: self.ctx,
            network,
            consensus: consensus.clone(),
            wal,
            sync,
            host,
        };

        let (node, handle) = self.node_spawner.spawn(node_deps).await?;

        let handle = EngineHandle {
            actor: node,
            handle,
        };

        Ok(handle)
    }
}

pub struct EngineBuilderWithHost<Node, Ctx, WalCodec, NetCodec>
where
    Ctx: Context,
    Node: node::Node<Context = Ctx>,
    WalCodec: codec::WalCodec<Ctx> + Clone,
    NetCodec: codec::ConsensusCodec<Ctx> + codec::SyncCodec<Ctx> + Clone,
{
    builder: EngineBuilder<Node, Ctx, WalCodec, NetCodec>,
    host: Box<dyn HostSpawner<Ctx>>,
}

impl<Node, Ctx, WalCodec, NetCodec> EngineBuilderWithHost<Node, Ctx, WalCodec, NetCodec>
where
    Ctx: Context,
    Node: node::Node<Context = Ctx>,
    WalCodec: codec::WalCodec<Ctx> + Clone,
    NetCodec: codec::ConsensusCodec<Ctx> + codec::SyncCodec<Ctx> + Clone,
{
    pub async fn build(self) -> Result<EngineHandle> {
        self.builder.build(self.host).await
    }
}
