pub mod auth;
pub mod config;
pub mod cursor;
pub mod error;
pub mod payload;
pub mod remote;
pub mod scan;

// Re-export the main types
pub use auth::LgtvAuth;
pub use cursor::LgtvCursor;
pub use error::{LgtvError, Result};
pub use remote::LgtvRemote;
pub use scan::{scan_for_tvs, TvDevice};
