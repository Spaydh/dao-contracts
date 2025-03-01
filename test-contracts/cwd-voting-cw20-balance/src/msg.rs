use cosmwasm_schema::{cw_serde, QueryResponses};

use cw20::Cw20Coin;
use cw20_base::msg::InstantiateMarketingInfo;

use cwd_macros::{info_query, token_query, voting_query};

#[cw_serde]
pub enum TokenInfo {
    Existing {
        address: String,
    },
    New {
        code_id: u64,
        label: String,

        name: String,
        symbol: String,
        decimals: u8,
        initial_balances: Vec<Cw20Coin>,
        marketing: Option<InstantiateMarketingInfo>,
    },
}

#[cw_serde]
pub struct InstantiateMsg {
    pub token_info: TokenInfo,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[token_query]
#[voting_query]
#[info_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
