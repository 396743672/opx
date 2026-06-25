pub use super::types::SoftwareProvider;

pub mod mysql;
pub mod jdk;
pub mod redis;
pub mod nginx;

pub use self::mysql::MysqlProvider;
pub use self::jdk::JdkProvider;
pub use self::redis::RedisProvider;
pub use self::nginx::NginxProvider;