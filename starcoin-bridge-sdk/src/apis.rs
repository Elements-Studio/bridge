// APIs module for Starcoin SDK compatibility
#![allow(dead_code, unused_variables)]

use anyhow::Result;
use futures::Stream;
use starcoin_bridge_json_rpc_types::Coin;

// Coin Read API stub
pub struct CoinReadApi {
    // TODO: Implement Starcoin coin/token reading
}

impl CoinReadApi {
    pub fn new() -> Self {
        Self {}
    }

    // Get total supply for a coin type
    pub async fn get_total_supply(
        &self,
        _coin_type: &str,
    ) -> Result<starcoin_bridge_json_rpc_types::Supply> {
        // TODO: Call Starcoin bridge RPC to get token total supply
        // For now return a placeholder
        Ok(starcoin_bridge_json_rpc_types::Supply { value: 0 })
    }

    // Get coins for an address
    pub async fn get_coins(
        &self,
        _address: [u8; 32],
        _coin_type: Option<String>,
        _cursor: Option<String>,
        _limit: Option<usize>,
    ) -> Result<starcoin_bridge_json_rpc_types::CoinPage> {
        // Return empty page for now
        Ok(starcoin_bridge_json_rpc_types::CoinPage {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        })
    }

    // Select coins up to a certain amount
    pub async fn select_coins(
        &self,
        _address: [u8; 32],
        _coin_type: Option<String>,
        _amount: u128,
        _exclude: Vec<[u8; 32]>,
    ) -> Result<Vec<Coin>> {
        // Return empty list for now
        Ok(vec![])
    }

    // Get a stream of coins for an address
    pub fn get_coins_stream(
        &self,
        _address: [u8; 32],
        _coin_type: Option<String>,
    ) -> impl Stream<Item = Result<Coin>> {
        // Return an empty stream for now
        futures::stream::empty()
    }
}

impl Default for CoinReadApi {
    fn default() -> Self {
        Self::new()
    }
}
