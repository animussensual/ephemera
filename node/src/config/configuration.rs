use std::io::Write;
use std::path::PathBuf;
use thiserror::Error;

use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Configuration {
    pub node_config: NodeConfig,
    pub quorum: BroadcastProtocolSettings,
    pub libp2p: Libp2pSettings,
    pub db_config: DbConfig,
    pub ws_config: WsConfig,
    pub network_client_listener_config: NetworkClientListenerConfig,
    pub http_config: HttpConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NodeConfig {
    pub address: String,
    pub pub_key: String,
    //TODO: clear memory after use
    pub priv_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BroadcastProtocolSettings {
    pub quorum_threshold_size: usize,
    pub cluster_size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Libp2pSettings {
    pub topic_name: String,
    pub heartbeat_interval_sec: u64,
    pub peers: Vec<PeerSetting>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PeerSetting {
    pub name: String,
    pub address: String,
    pub pub_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DbConfig {
    pub db_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WsConfig {
    pub ws_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NetworkClientListenerConfig {
    pub address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HttpConfig {
    pub address: String,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("Configuration file exists: '{}'", .0)]
    ConfigurationFileExists(String),
    #[error("Configuration file does not exists: '{}'", .0)]
    ConfigurationFileDoesNotExists(String),
    #[error("Configuration file does not exist")]
    IoError(#[from] std::io::Error),
    #[error("{}", .0)]
    Other(String),
}

const EPHEMERA_DIR_NAME: &str = ".ephemera";
const EPHEMERA_CONFIG_FILE: &str = "ephemera.toml";

type Result<T> = std::result::Result<T, ConfigurationError>;

impl Configuration {
    pub fn try_load(file: PathBuf) -> Result<Configuration> {
        let config = config::Config::builder()
            .add_source(config::File::from(file))
            .build()
            .map_err(|e| ConfigurationError::Other(e.to_string()))?;

        Ok(config
            .try_deserialize()
            .map_err(|e| ConfigurationError::Other(e.to_string()))?)
    }

    pub fn try_load_node(node_name: &str, file: &str) -> Result<Configuration> {
        let file_path = Self::ephemera_node_dir(node_name)?.join(file);

        Configuration::try_load(file_path)
    }

    pub fn try_load_from_home_dir(node_name: &str) -> Result<Configuration> {
        let file_path = Configuration::ephemera_config_file(node_name)?;
        let config = config::Config::builder()
            .add_source(config::File::from(file_path))
            .build()
            .map_err(|e| ConfigurationError::Other(e.to_string()))?;

        Ok(config
            .try_deserialize()
            .map_err(|e| ConfigurationError::Other(e.to_string()))?)
    }

    pub fn try_create(&self, node_name: &str) -> Result<()> {
        let conf_path = Configuration::ephemera_node_dir(node_name)?;
        if !conf_path.exists() {
            std::fs::create_dir_all(conf_path)?;
        }

        let file_path = Configuration::ephemera_config_file(node_name)?;
        if file_path.exists() {
            return Err(ConfigurationError::ConfigurationFileExists(
                file_path.to_str().unwrap().to_string(),
            ));
        }

        self.write(file_path)?;
        Ok(())
    }

    pub fn try_update(&self, node_name: &str) -> Result<()> {
        let file_path = Configuration::ephemera_config_file(node_name)?;
        if !file_path.exists() {
            log::error!(
                "Configuration file does not exist {}",
                file_path.to_str().unwrap()
            );
            return Err(ConfigurationError::ConfigurationFileDoesNotExists(
                file_path.to_str().unwrap().to_string(),
            ));
        }
        self.write(file_path)?;
        Ok(())
    }

    fn ephemera_dir() -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .map(|home| home.join(EPHEMERA_DIR_NAME))
            .ok_or(ConfigurationError::Other(
                "Could not find home directory".to_string(),
            ))?)
    }

    fn ephemera_node_dir(node_name: &str) -> Result<PathBuf> {
        Ok(Self::ephemera_dir()?.join(node_name))
    }

    pub fn ephemera_config_file(node_name: &str) -> Result<PathBuf> {
        Ok(Self::ephemera_node_dir(node_name)?.join(EPHEMERA_CONFIG_FILE))
    }

    fn write(&self, file_path: PathBuf) -> Result<()> {
        let config = toml::to_string(&self)
            .map_err(|e| ConfigurationError::Other(format!("Failed to serialize configuration: {}", e)))?;

        let config = format!(
            "#This file is generated by cli and automatically overwritten every time when cli is run\n{}",
            config
        );

        log::info!("Writing configuration to file: '{}'", file_path.display());
        let mut file = std::fs::File::create(&file_path)?;
        file.write_all(config.as_bytes())?;

        Ok(())
    }
}
