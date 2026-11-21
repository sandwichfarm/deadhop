//! deadhop: on-chain bitcoin payments served only over ContextVM (MCP over Nostr).
pub mod app;
pub mod config;
pub mod db;
pub mod electrum;
pub mod error;
pub mod handshake;
pub mod keyholder;
pub mod manage;
pub mod mcp;
pub mod pow;
pub mod privacy;
pub mod ratelimit;
pub mod relay;
pub mod relayhealth;
pub mod service;
pub mod settings;
pub mod wallet;
