use clap::Parser;
use color_eyre::eyre;
use tracing::info;

use malachitebft_app::Node;
use malachitebft_config::MetricsConfig;

use crate::metrics;

#[derive(Parser, Debug, Clone, Default, PartialEq)]
pub struct StartCmd {
    #[clap(long)]
    pub start_height: Option<u64>,
    
    /// Address of the staking contract for dynamic validator sets
    #[clap(long)]
    pub staking_contract: Option<String>,
    
    /// Host for the execution layer (default: localhost)
    #[clap(long, default_value = "localhost")]
    pub el_host: String,
    
    /// Port offset for execution layer ports (default: 0)
    #[clap(long, default_value = "0")]
    pub el_port_offset: u16,
    
    /// Path to JWT secret file for engine API authentication
    #[clap(long, default_value = "./assets/jwtsecret")]
    pub jwt_secret_path: String,
}

impl StartCmd {
    pub async fn run(&self, node: impl Node, metrics: Option<MetricsConfig>) -> eyre::Result<()> {
        info!("Node is starting...");

        start(node, metrics).await?;

        info!("Node has stopped");

        Ok(())
    }
}

/// start command to run a node.
pub async fn start(node: impl Node, metrics: Option<MetricsConfig>) -> eyre::Result<()> {
    // Enable Prometheus
    if let Some(metrics) = metrics {
        if metrics.enabled {
            tokio::spawn(metrics::serve(metrics.listen_addr));
        }
    }

    // Start the node
    node.run().await?;

    Ok(())
}
