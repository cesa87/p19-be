//! Solana Sniper Bot Module
//! 
//! Provides fast token sniping capabilities for PumpFun and other Solana DEXes.
//! Features:
//! - Wallet session management
//! - Token lookup and analysis
//! - PumpFun integration
//! - Fast execution with Jito bundles (future)
//! - Position tracking and auto-sell

pub mod pumpfun;
pub mod solana;
pub mod wallet;

pub use pumpfun::{PumpFunClient, PumpToken, PriceQuote};
pub use solana::SolanaClient;
pub use wallet::WalletSession;
