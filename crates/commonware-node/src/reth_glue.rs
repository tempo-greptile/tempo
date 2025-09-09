//! Glue to run a reth node inside a commonware runtime context.
//!
//! None of the code here is actually specific to commonware-xyz. It just so
//! happens that in order to spawn a reth instance, all that is needed is a
//! [`reth_tasks::TaskManager`] instance, which can be done from inside any
//! tokio runtime instance.
//!
//! # Why this exists
//!
//! The peculiarity of commonwarexyz is that it fully wraps a tokio runtime and
//! passes an abstracted `S: Spawner` into all code that needs to spawn tasks.
//! So rather than using [`tokio::runtime::Handle::spawn`] it uses
//! `<S as Spawner>::spawn` to run async tasks on the runtime while tracking
//! the amount and (named) context of all tasks.
//!
//! In a similar manner, reth's primary way of launching nodes is through a
//! [`reth_cli_runner::CliRunner`], which also takes possession of a tokio
//! runtime. However, it then uses a tokio runtime's *handle* to construct a
//! `[reth_tasks::TaskManager]` and [`reth_tasks::TaskExecutor`], passing the
//! latter to through its stack to spawn tasks (and track tasks, etc).

use std::{sync::Arc, time::Duration};

use commonware_runtime::{Runner as _, Spawner as _};
use reth::CliContext;
use reth_chainspec::Hardforks;
use reth_cli::chainspec::ChainSpecParser;
use reth_cli_commands::{
    NodeCommand,
    common::{CliComponentsBuilder, CliHeader, CliNodeTypes},
    launcher::FnLauncher,
};
use reth_db::DatabaseEnv;
use reth_ethereum_cli::interface::Commands;
use reth_node_builder::{NodeBuilder, NodePrimitives, WithLaunchContext};
use reth_tasks::TaskManager;
use tokio::signal::unix::SignalKind;
use tracing::{debug, error, trace};

use crate::cli::TempoSpecificArgs;

/// This is a hack to get the commonware context into reth's FnLauncher.
// TODO: I hate this. We should remove it.
#[derive(clap::Args)]
pub struct ContextEnrichedArgs<TArgs: clap::Args> {
    #[clap(skip)]
    pub context: Option<commonware_runtime::tokio::Context>,
    #[clap(flatten)]
    pub args: TArgs,
}

impl<TArgs: std::fmt::Debug + clap::Args> std::fmt::Debug for ContextEnrichedArgs<TArgs> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextEnrichedArgs")
            .field(
                "context",
                match &self.context {
                    Some(_) => &"Some(<commonware specific runtime context>)",
                    None => &"None",
                },
            )
            .field("args", &self.args)
            .finish()
    }
}

/// Execute the provided cli `args` inside `runner`. `components` and `launcher`
/// are command specific configurations.
///
/// This is essentially [`reth_ethereum_cli::Cli::with_runner_and_components`]
/// adapted to work with [`commonware_runtime::tokio::Runner`].
// TODO: this is an absolutely terrible name. Fix it.
pub(crate) fn with_runner_and_components<TChainSpecParser, TNode>(
    runner: commonware_runtime::tokio::Runner,
    args: Commands<TChainSpecParser, TempoSpecificArgs>,
    components: impl CliComponentsBuilder<TNode>,
    launcher: impl AsyncFnOnce(
        WithLaunchContext<NodeBuilder<Arc<DatabaseEnv>, TChainSpecParser::ChainSpec>>,
        ContextEnrichedArgs<TempoSpecificArgs>,
    ) -> eyre::Result<()>,
) -> eyre::Result<()>
where
    TNode: CliNodeTypes<Primitives: NodePrimitives<BlockHeader: CliHeader>, ChainSpec: Hardforks>,
    TChainSpecParser: ChainSpecParser<ChainSpec = TNode::ChainSpec>,
{
    // TODO: bring tracing back into this mess.
    //
    // // Add network name if available to the logs dir
    // if let Some(chain_spec) = self.command.chain_spec() {
    //     self.logs.log_file_directory = self
    //         .logs
    //         .log_file_directory
    //         .join(chain_spec.chain().to_string());
    // }
    // let _guard = self.init_tracing()?;
    // info!(target: "reth::cli", "Initialized tracing, debug log directory: {}", self.logs.log_file_directory);

    // // Install the prometheus recorder to be sure to record all metrics
    // let _ = install_prometheus_recorder();
    //
    // TODO: prepare this with some defaults if not available in the config file.
    // It's simply not necessary for very many of the other commands.
    //
    // let runner = commonware_runtime::tokio::Runner::new(runtime_config);

    match args {
        Commands::Node(command) => runner.run_command_until_exit(|commonware_ctx, reth_ctx| {
            inject_commonware_context_into_node_cmd(command, commonware_ctx).execute(
                reth_ctx,
                FnLauncher::new::<TChainSpecParser, ContextEnrichedArgs<TempoSpecificArgs>>(
                    launcher,
                ),
            )
        }),
        Commands::Init(command) => runner.run_blocking_until_ctrl_c(command.execute::<TNode>()),
        Commands::InitState(command) => {
            runner.run_blocking_until_ctrl_c(command.execute::<TNode>())
        }
        Commands::Import(command) => {
            runner.run_blocking_until_ctrl_c(command.execute::<TNode, _>(components))
        }
        Commands::ImportEra(command) => {
            runner.run_blocking_until_ctrl_c(command.execute::<TNode>())
        }
        Commands::ExportEra(command) => {
            runner.run_blocking_until_ctrl_c(command.execute::<TNode>())
        }
        Commands::DumpGenesis(command) => runner.run_blocking_until_ctrl_c(command.execute()),
        Commands::Db(command) => runner.run_blocking_until_ctrl_c(command.execute::<TNode>()),
        Commands::Download(command) => runner.run_blocking_until_ctrl_c(command.execute::<TNode>()),
        Commands::Stage(command) => {
            runner.run_command_until_exit(|_, ctx| command.execute::<TNode, _>(ctx, components))
        }
        Commands::P2P(command) => runner.run_until_ctrl_c(command.execute::<TNode>()),
        // #[cfg(feature = "dev")]
        // Commands::TestVectors(command) => runner.run_until_ctrl_c(command.execute()),
        Commands::Config(command) => runner.run_until_ctrl_c(command.execute()),
        Commands::Recover(command) => {
            runner.run_command_until_exit(|_, ctx| command.execute::<TNode>(ctx))
        }
        Commands::Prune(command) => runner.run_until_ctrl_c(command.execute::<TNode>()),
        Commands::ReExecute(command) => {
            runner.run_until_ctrl_c(command.execute::<TNode>(components))
        }
    }
}

/// Extension trait for [`commonware_runtime::tokio::Runner`] to have it
/// look like [`reth_cli_runner::CliRunner`].
trait CommonwareRunnerExt {
    fn run_blocking_until_ctrl_c<F, E>(self, fut: F) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Send + Sync + From<std::io::Error> + 'static;

    fn run_command_until_exit<F, E>(
        self,
        command: impl FnOnce(commonware_runtime::tokio::Context, CliContext) -> F,
    ) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>>,
        E: Send + Sync + From<std::io::Error> + From<reth_tasks::PanickedTaskError> + 'static;

    fn run_until_ctrl_c<F, E>(self, fut: F) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>>,
        E: Send + Sync + 'static + From<std::io::Error>;
}

impl CommonwareRunnerExt for commonware_runtime::tokio::Runner {
    fn run_blocking_until_ctrl_c<F, E>(self, fut: F) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Send + Sync + From<std::io::Error> + 'static,
    {
        // TODO: in commonware-wrapped tokio world there is no way to run
        // `block_on` on handle similar to what reth is doing. So we
        self.start(move |ctx| async move {
            let handle = ctx.spawn(move |_ctx| run_until_ctrl_c(fut));
            handle.await.expect("failed to join task")
        })?;

        // TODO: leaving this - taken straight from reth.
        //
        // We can't do the same thing here because commonware's `Runner::start`
        // takes the runtime by ownership. Would wrapping it in `ManuallyDrop`
        // work?

        // drop the tokio runtime on a separate thread because drop blocks until its pools
        // (including blocking pool) are shutdown. In other words `drop(tokio_runtime)` would block
        // the current thread but we want to exit right away.
        // std::thread::Builder::new()
        //     .name("tokio-runtime-shutdown".to_string())
        //     .spawn(move || drop(self))
        //     .unwrap();

        Ok(())
    }

    fn run_command_until_exit<F, E>(
        self,
        command: impl FnOnce(commonware_runtime::tokio::Context, CliContext) -> F,
    ) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>>,
        E: Send + Sync + From<std::io::Error> + From<reth_tasks::PanickedTaskError> + 'static,
    {
        self.start(move |ctx| async {
            let mut reth_task_manager = TaskManager::current();
            let reth_cli_context = reth::CliContext {
                task_executor: reth_task_manager.executor(),
            };

            let res = run_to_completion_or_panic(
                &mut reth_task_manager,
                run_until_ctrl_c(command(ctx, reth_cli_context)),
            )
            .await;

            if res.is_err() {
                error!(target: "tempo::cli", "shutting down due to error");
            } else {
                debug!(target: "tempo::cli", "shutting down gracefully");
                // after the command has finished or exit signal was received we shutdown the task
                // manager which fires the shutdown signal to all tasks spawned via the task
                // executor and awaiting on tasks spawned with graceful shutdown
                reth_task_manager.graceful_shutdown_with_timeout(Duration::from_secs(5));
            }

            res
        })

        // TODO: leaving this - taken straight from reth.
        //
        // We can't do the same thing here because commonware's `Runner::start`
        // takes the runtime by ownership. Would wrapping it in `ManuallyDrop`
        // work?

        // let (tx, rx) = mpsc::channel();
        // std::thread::Builder::new()
        //     .name("tokio-runtime-shutdown".to_string())
        //     .spawn(move || {
        //         drop(tokio_runtime);
        //         let _ = tx.send(());
        //     })
        //     .unwrap();

        // let _ = rx.recv_timeout(Duration::from_secs(5)).inspect_err(|err| {
        //     debug!(target: "reth::cli", %err, "tokio runtime shutdown timed out");
        // });
    }

    fn run_until_ctrl_c<F, E>(self, fut: F) -> Result<(), E>
    where
        F: Future<Output = Result<(), E>>,
        E: Send + Sync + 'static + From<std::io::Error>,
    {
        self.start(move |_ctx| run_until_ctrl_c(fut))
    }
}

async fn run_to_completion_or_panic<F, E>(tasks: &mut TaskManager, fut: F) -> Result<(), E>
where
    F: Future<Output = Result<(), E>>,
    E: Send + Sync + From<reth_tasks::PanickedTaskError> + 'static,
{
    {
        tokio::select! {
            task_manager_result = tasks => {
                if let Err(panicked_error) = task_manager_result {
                    return Err(panicked_error.into());
                }
            },
            res = fut => res?,
        }
    }
    Ok(())
}

async fn run_until_ctrl_c<F, E>(fut: F) -> Result<(), E>
where
    F: Future<Output = Result<(), E>>,
    E: Send + Sync + 'static + From<std::io::Error>,
{
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())
        .expect("setting a SIGINT listener should always work on unix; is this running on unix?");
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("setting a SIGTERM listener should always work on unix; is this running on unix?");

    // TODO: put these `trace!` macros into a span.
    tokio::select! {
        _ = sigint.recv() => {
            trace!("received SIGINT");
        }

        _ = sigterm.recv() => {
            trace!("received SIGTERM");
        }

        res = fut => res?,
    }
    Ok(())
}

fn inject_commonware_context_into_node_cmd<TChainSpecParser: ChainSpecParser>(
    node_cmd: Box<NodeCommand<TChainSpecParser, TempoSpecificArgs>>,
    commonware_context: commonware_runtime::tokio::Context,
) -> Box<NodeCommand<TChainSpecParser, ContextEnrichedArgs<TempoSpecificArgs>>> {
    let NodeCommand {
        config,
        chain,
        metrics,
        instance,
        with_unused_ports,
        datadir,
        network,
        rpc,
        txpool,
        builder,
        debug,
        db,
        dev,
        pruning,
        engine,
        era,
        ext,
    } = *node_cmd;
    NodeCommand {
        config,
        chain,
        metrics,
        instance,
        with_unused_ports,
        datadir,
        network,
        rpc,
        txpool,
        builder,
        debug,
        db,
        dev,
        pruning,
        engine,
        era,
        ext: ContextEnrichedArgs {
            context: Some(commonware_context),
            args: ext,
        },
    }
    .into()
}
