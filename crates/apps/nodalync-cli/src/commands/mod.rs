//! CLI command implementations.

pub mod balance;
pub mod channel;
pub mod completions;
pub mod content;
pub mod deposit;
pub mod earnings;
pub mod init;
pub mod mcp_server;
pub mod query;
pub mod search;
pub mod settle;
pub mod start;
pub mod status;
pub mod stop;
pub mod whoami;
pub mod withdraw;

// Content command modules keep their `commands::<name>` paths
pub use content::{
    build_l2, delete, list, merge_l2, preview, publish, reference, synthesize, update, versions,
    visibility,
};

// Re-export command handlers
pub use balance::balance;
pub use build_l2::build_l2;
pub use channel::{close_channel, dispute_channel, list_channels, open_channel, resolve_dispute};
pub use completions::completions;
pub use delete::delete;
pub use deposit::deposit;
pub use earnings::earnings;
pub use init::init;
pub use list::list;
pub use mcp_server::mcp_server;
pub use merge_l2::merge_l2;
pub use preview::preview;
pub use publish::publish;
pub use query::query;
pub use reference::reference;
pub use search::search;
pub use settle::settle;
pub use start::{start, start_daemon_sync};
pub use status::status;
pub use stop::stop;
pub use synthesize::synthesize;
pub use update::update;
pub use versions::versions;
pub use visibility::visibility;
pub use whoami::whoami;
pub use withdraw::withdraw;
