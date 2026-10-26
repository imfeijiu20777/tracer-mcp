use std::{env, path::PathBuf};
use anyhow::Result;

pub mod framed;
pub mod payload;

pub const MESSAGE_ALPN: &[u8] = b"zline/message/1";

/// 运行角色：区分 server 和 client
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Server,
    Client,
}

impl Role {
    /// 数据目录对应的环境变量名
    pub fn data_path_env(self) -> &'static str {
        match self {
            Role::Server => "TRACET_SERVER_DATA_PATH",
            Role::Client => "TRACET_CLIENT_DATA_PATH",
        }
    }

    /// 密钥路径对应的环境变量名
    pub fn secret_key_env(self) -> &'static str {
        match self {
            Role::Server => "TRACET_SERVER_SECRET_KEY",
            Role::Client => "TRACET_CLIENT_SECRET_KEY",
        }
    }

    /// 默认密钥文件名（放在 cwd 下）
    pub fn secret_key_file(self) -> &'static str {
        match self {
            Role::Server => "tracer_server_secret_key.key",
            Role::Client => "tracer_client_secret_key.key",
        }
    }

    /// 默认数据目录名（放在 cwd 下）
    pub fn data_dir_name(self) -> &'static str {
        match self {
            Role::Server => "tracer_server_data",
            Role::Client => "tracer_client_data",
        }
    }

    /// ProjectDirs 用的应用名（让 server/client 配置目录分开）
    pub fn app_name(self) -> &'static str {
        match self {
            Role::Server => "tracert-server",
            Role::Client => "tracert-client",
        }
    }
}

/// 支持从字符串解析（方便接命令行参数，如 clap 的 value_parser）
impl std::str::FromStr for Role {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "server" => Ok(Role::Server),
            "client" => Ok(Role::Client),
            other => Err(anyhow::anyhow!("unknown role: {other}, expected server|client")),
        }
    }
}

/// 数据路径：优先环境变量，否则 cwd/tracer_{server|client}_data
pub fn get_data_path(role: Role) -> PathBuf {
    env::var_os(role.data_path_env())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(role.data_dir_name())
        })
}

/// 密钥路径：优先环境变量，否则 cwd/tracer_{server|client}_secret_key.key
pub fn get_secret_path(role: Role) -> PathBuf {
    env::var_os(role.secret_key_env())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(role.secret_key_file())
        })
}