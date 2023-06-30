mod faucet;
pub(crate) use crate::faucet::*;

mod web;
pub(crate) use web::*;

mod discord;
pub use discord::*;
// TODO
// - [X] Funding on startup.
// - [X] Webserver
// - [X] Separate web server for faucet for easier testing?
// - [X} Healthcheck
// - [X] Move receipt processing into separate method.
// - [X] Only process receipts of tracked transfers
// - Keep track of processed blocks.
// - Add method of fetching missed blocks (or just query all pending transactions for receipts on startup?) and process them.
// - Error handling
