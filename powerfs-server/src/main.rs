use powerfs_common::{
    types::{NodeId, ClusterConfig, VolumeId},
    utils::{generate_node_id, generate_volume_id},
    error::{PowerFsError, Result},
};
use powerfs_core::storage::StorageManager;
use powerfs_master::master::MasterNode;
use powerfs_fuse::fuse::FuseClient;
use tokio;
use clap::{Parser, Subcommand};
use log::{info, warn, error};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "powerfs", version = "0.1.0", about = "PowerFS - Zero-jitter unified parallel file system")]
struct Cli {
    #[arg(long, default_value = "info")]
    log_level: String,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Master {
        #[arg(long, default_value = "127.0.0.1:9333")]
        address: String,
        
        #[arg(long)]
        data_path: Option<String>,
    },
    
    Volume {
        #[arg(long, default_value = "127.0.0.1:8080")]
        address: String,
        
        #[arg(long)]
        master_address: String,
        
        #[arg(long)]
        data_path: Option<String>,
    },
    
    Fuse {
        #[arg(long)]
        mount_point: String,
        
        #[arg(long)]
        master_address: Option<String>,
    },
    
    CreateVolume {
        #[arg(long)]
        master_address: String,
        
        #[arg(long)]
        node_id: String,
        
        #[arg(long, default_value = "1073741824")]
        size: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    env_logger::Builder::new()
        .filter_level(match cli.log_level.as_str() {
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Info,
        })
        .init();
    
    match cli.command {
        Commands::Master { address, data_path } => {
            run_master(&address, data_path).await
        }
        
        Commands::Volume { address, master_address, data_path } => {
            run_volume(&address, &master_address, data_path).await
        }
        
        Commands::Fuse { mount_point, master_address } => {
            run_fuse(&mount_point, master_address).await
        }
        
        Commands::CreateVolume { master_address, node_id, size } => {
            create_volume(&master_address, &node_id, size).await
        }
    }
}

async fn run_master(address: &str, data_path: Option<String>) -> Result<()> {
    info!("Starting PowerFS Master node");
    
    let data_dir = data_path.unwrap_or_else(|| "./data/master".to_string());
    std::fs::create_dir_all(&data_dir)?;
    
    let master = MasterNode::new(address, None).await?;
    
    info!("Master node initialized: {:?}", master.id());
    info!("Listening on: {}", address);
    
    master.start().await?;
    
    Ok(())
}

async fn run_volume(address: &str, master_address: &str, data_path: Option<String>) -> Result<()> {
    info!("Starting PowerFS Volume node");
    
    let node_id = generate_node_id();
    let data_dir = data_path.unwrap_or_else(|| "./data/volume".to_string());
    std::fs::create_dir_all(&data_dir)?;
    
    let storage_manager = Arc::new(StorageManager::new(node_id.clone(), data_dir));
    
    storage_manager.load_volumes()?;
    
    info!("Volume node initialized: {:?}", node_id);
    info!("Connected to master: {}", master_address);
    
    tokio::signal::ctrl_c().await?;
    
    Ok(())
}

async fn run_fuse(mount_point: &str, master_address: Option<String>) -> Result<()> {
    info!("Starting PowerFS FUSE client");
    
    let node_id = generate_node_id();
    let storage_manager = Arc::new(StorageManager::new(node_id, "./data/fuse".to_string()));
    
    storage_manager.load_volumes()?;
    
    let fuse_client = FuseClient::new(storage_manager, mount_point);
    
    info!("Mounting PowerFS at: {}", mount_point);
    
    if let Err(e) = fuse_client.mount().await {
        error!("Failed to mount FUSE: {}", e);
        return Err(e);
    }
    
    tokio::signal::ctrl_c().await?;
    
    fuse_client.unmount().await?;
    
    Ok(())
}

async fn create_volume(master_address: &str, node_id: &str, size: u64) -> Result<()> {
    info!("Creating volume on node: {}", node_id);
    
    Ok(())
}
