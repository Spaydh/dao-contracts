use cosmwasm_std::{to_binary, Addr, Coin, Empty, Uint128};
use cw20::Cw20Coin;

use cw_multi_test::{next_block, App, BankSudo, ContractWrapper, Executor, SudoMsg};
use cw_utils::Duration;
use cwd_interface::{Admin, ModuleInstantiateInfo};
use cwd_pre_propose_multiple as cppm;

use cwd_testing::contracts::{
    cw20_balances_voting_contract, cw20_contract, cw20_stake_contract,
    cw20_staked_balances_voting_contract, cw4_contract, cw721_contract, cwd_core_contract,
    native_staked_balances_voting_contract, pre_propose_single_contract,
};
use cwd_voting::{
    deposit::{DepositRefundPolicy, UncheckedDepositInfo},
    multiple_choice::VotingStrategy,
    pre_propose::PreProposeInfo,
    threshold::PercentageThreshold,
};
use cwd_voting_cw20_staked::msg::ActiveThreshold;

use crate::{
    msg::InstantiateMsg, testing::tests::proposal_multiple_contract, testing::tests::CREATOR_ADDR,
};

#[allow(dead_code)]
fn get_pre_propose_info(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> PreProposeInfo {
    let pre_propose_contract = app.store_code(pre_propose_single_contract());
    PreProposeInfo::ModuleMayPropose {
        info: ModuleInstantiateInfo {
            code_id: pre_propose_contract,
            msg: to_binary(&cppm::InstantiateMsg {
                deposit_info,
                open_proposal_submission,
                extension: Empty::default(),
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "pre_propose_contract".to_string(),
        },
    }
}

fn _get_default_token_dao_proposal_module_instantiate(app: &mut App) -> InstantiateMsg {
    let quorum = PercentageThreshold::Majority {};
    let voting_strategy = VotingStrategy::SingleChoice { quorum };

    InstantiateMsg {
        voting_strategy,
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(
            app,
            Some(UncheckedDepositInfo {
                denom: cwd_voting::deposit::DepositToken::VotingModuleToken {},
                amount: Uint128::new(10_000_000),
                refund_policy: DepositRefundPolicy::OnlyPassed,
            }),
            false,
        ),
        close_proposal_on_execution_failure: true,
    }
}

// Same as above but no proposal deposit.
fn _get_default_non_token_dao_proposal_module_instantiate(app: &mut App) -> InstantiateMsg {
    let quorum = PercentageThreshold::Majority {};
    let voting_strategy = VotingStrategy::SingleChoice { quorum };

    InstantiateMsg {
        voting_strategy,
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(app, None, false),
        close_proposal_on_execution_failure: true,
    }
}

pub fn _instantiate_with_staked_cw721_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    let initial_balances: Vec<Cw20Coin> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|Cw20Coin { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let cw721_id = app.store_code(cw721_contract());
    let cw721_stake_id = app.store_code({
        let contract = ContractWrapper::new(
            cwd_voting_cw721_staked::contract::execute,
            cwd_voting_cw721_staked::contract::instantiate,
            cwd_voting_cw721_staked::contract::query,
        );
        Box::new(contract)
    });
    let core_contract_id = app.store_code(cwd_core_contract());

    let nft_address = app
        .instantiate_contract(
            cw721_id,
            Addr::unchecked("ekez"),
            &cw721_base::msg::InstantiateMsg {
                minter: "ekez".to_string(),
                symbol: "token".to_string(),
                name: "ekez token best token".to_string(),
            },
            &[],
            "nft-staking",
            None,
        )
        .unwrap();

    let instantiate_core = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: cw721_stake_id,
            msg: to_binary(&cwd_voting_cw721_staked::msg::InstantiateMsg {
                owner: Some(Admin::CoreModule {}),
                manager: None,
                unstaking_duration: None,
                nft_address: nft_address.to_string(),
            })
            .unwrap(),
            admin: None,
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            label: "DAO DAO governance module.".to_string(),
            admin: Some(Admin::CoreModule {}),
            msg: to_binary(&proposal_module_instantiate).unwrap(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    let core_addr = app
        .instantiate_contract(
            core_contract_id,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let core_state: cwd_core::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(core_addr.clone(), &cwd_core::msg::QueryMsg::DumpState {})
        .unwrap();
    let staking_addr = core_state.voting_module;

    for Cw20Coin { address, amount } in initial_balances {
        for i in 0..amount.u128() {
            app.execute_contract(
                Addr::unchecked("ekez"),
                nft_address.clone(),
                &cw721_base::msg::ExecuteMsg::<Option<Empty>, Empty>::Mint(
                    cw721_base::msg::MintMsg::<Option<Empty>> {
                        token_id: format!("{address}_{i}"),
                        owner: address.clone(),
                        token_uri: None,
                        extension: None,
                    },
                ),
                &[],
            )
            .unwrap();
            app.execute_contract(
                Addr::unchecked(address.clone()),
                nft_address.clone(),
                &cw721_base::msg::ExecuteMsg::<Option<Empty>, Empty>::SendNft {
                    contract: staking_addr.to_string(),
                    token_id: format!("{address}_{i}"),
                    msg: to_binary("").unwrap(),
                },
                &[],
            )
            .unwrap();
        }
    }

    // Update the block so that staked balances appear.
    app.update_block(|block| block.height += 1);

    core_addr
}

pub fn _instantiate_with_native_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<Cw20Coin> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|Cw20Coin { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let native_stake_id = app.store_code(native_staked_balances_voting_contract());
    let core_contract_id = app.store_code(cwd_core_contract());

    let instantiate_core = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: native_stake_id,
            msg: to_binary(&cwd_voting_native_staked::msg::InstantiateMsg {
                owner: Some(Admin::CoreModule {}),
                manager: None,
                denom: "ujuno".to_string(),
                unstaking_duration: None,
            })
            .unwrap(),
            admin: None,
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            label: "DAO DAO governance module.".to_string(),
            admin: Some(Admin::CoreModule {}),
            msg: to_binary(&proposal_module_instantiate).unwrap(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    let core_addr = app
        .instantiate_contract(
            core_contract_id,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: cwd_core::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(core_addr.clone(), &cwd_core::msg::QueryMsg::DumpState {})
        .unwrap();
    let native_staking_addr = gov_state.voting_module;

    for Cw20Coin { address, amount } in initial_balances {
        app.sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: address.clone(),
            amount: vec![Coin {
                denom: "ujuno".to_string(),
                amount,
            }],
        }))
        .unwrap();
        app.execute_contract(
            Addr::unchecked(&address),
            native_staking_addr.clone(),
            &cwd_voting_native_staked::msg::ExecuteMsg::Stake {},
            &[Coin {
                amount,
                denom: "ujuno".to_string(),
            }],
        )
        .unwrap();
    }

    app.update_block(next_block);

    core_addr
}

pub fn instantiate_with_cw20_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());

    let cw20_id = app.store_code(cw20_contract());
    let core_id = app.store_code(cwd_core_contract());
    let votemod_id = app.store_code(cw20_balances_voting_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<Cw20Coin> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|Cw20Coin { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let governance_instantiate = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: true,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_id,
            msg: to_binary(&cwd_voting_cw20_balance::msg::InstantiateMsg {
                token_info: cwd_voting_cw20_balance::msg::TokenInfo::New {
                    code_id: cw20_id,
                    label: "DAO DAO governance token".to_string(),
                    name: "DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances,
                    marketing: None,
                },
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            label: "DAO DAO governance module.".to_string(),
            admin: Some(Admin::CoreModule {}),
            msg: to_binary(&proposal_module_instantiate).unwrap(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    app.instantiate_contract(
        core_id,
        Addr::unchecked(CREATOR_ADDR),
        &governance_instantiate,
        &[],
        "DAO DAO",
        None,
    )
    .unwrap()
}

pub fn instantiate_with_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<Cw20Coin> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|Cw20Coin { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let cw20_id = app.store_code(cw20_contract());
    let cw20_stake_id = app.store_code(cw20_stake_contract());
    let staked_balances_voting_id = app.store_code(cw20_staked_balances_voting_contract());
    let core_contract_id = app.store_code(cwd_core_contract());

    let instantiate_core = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: staked_balances_voting_id,
            msg: to_binary(&cwd_voting_cw20_staked::msg::InstantiateMsg {
                active_threshold: None,
                token_info: cwd_voting_cw20_staked::msg::TokenInfo::New {
                    code_id: cw20_id,
                    label: "DAO DAO governance token.".to_string(),
                    name: "DAO DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances: initial_balances.clone(),
                    marketing: None,
                    staking_code_id: cw20_stake_id,
                    unstaking_duration: Some(Duration::Height(6)),
                    initial_dao_balance: None,
                },
            })
            .unwrap(),
            admin: None,
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            label: "DAO DAO governance module.".to_string(),
            admin: Some(Admin::CoreModule {}),
            msg: to_binary(&proposal_module_instantiate).unwrap(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    let core_addr = app
        .instantiate_contract(
            core_contract_id,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: cwd_core::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(core_addr.clone(), &cwd_core::msg::QueryMsg::DumpState {})
        .unwrap();
    let voting_module = gov_state.voting_module;

    let staking_contract: Addr = app
        .wrap()
        .query_wasm_smart(
            voting_module.clone(),
            &cwd_voting_cw20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();
    let token_contract: Addr = app
        .wrap()
        .query_wasm_smart(
            voting_module,
            &cwd_interface::voting::Query::TokenContract {},
        )
        .unwrap();

    // Stake all the initial balances.
    for Cw20Coin { address, amount } in initial_balances {
        app.execute_contract(
            Addr::unchecked(address),
            token_contract.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: staking_contract.to_string(),
                amount,
                msg: to_binary(&cw20_stake::msg::ReceiveMsg::Stake {}).unwrap(),
            },
            &[],
        )
        .unwrap();
    }

    // Update the block so that those staked balances appear.
    app.update_block(|block| block.height += 1);

    core_addr
}

pub fn instantiate_with_staking_active_threshold(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
    active_threshold: Option<ActiveThreshold>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());
    let cw20_id = app.store_code(cw20_contract());
    let cw20_staking_id = app.store_code(cw20_stake_contract());
    let core_id = app.store_code(cwd_core_contract());
    let votemod_id = app.store_code(cw20_staked_balances_voting_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    let governance_instantiate = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: true,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_id,
            msg: to_binary(&cwd_voting_cw20_staked::msg::InstantiateMsg {
                token_info: cwd_voting_cw20_staked::msg::TokenInfo::New {
                    code_id: cw20_id,
                    label: "DAO DAO governance token".to_string(),
                    name: "DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances,
                    marketing: None,
                    staking_code_id: cw20_staking_id,
                    unstaking_duration: None,
                    initial_dao_balance: None,
                },
                active_threshold,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    app.instantiate_contract(
        core_id,
        Addr::unchecked(CREATOR_ADDR),
        &governance_instantiate,
        &[],
        "DAO DAO",
        None,
    )
    .unwrap()
}

pub fn _instantiate_with_cw4_groups_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_weights: Option<Vec<Cw20Coin>>,
) -> Addr {
    let proposal_module_code_id = app.store_code(proposal_multiple_contract());
    let cw4_id = app.store_code(cw4_contract());
    let core_id = app.store_code(cwd_core_contract());
    let votemod_id = app.store_code(cw4_contract());

    let initial_weights = initial_weights.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(1),
        }]
    });

    // Remove duplicates so that we can test duplicate voting.
    let initial_weights: Vec<cw4::Member> = {
        let mut already_seen = vec![];
        initial_weights
            .into_iter()
            .filter(|Cw20Coin { address, .. }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .map(|Cw20Coin { address, amount }| cw4::Member {
                addr: address,
                weight: amount.u128() as u64,
            })
            .collect()
    };

    let governance_instantiate = cwd_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: true,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_id,
            msg: to_binary(&cwd_voting_cw4::msg::InstantiateMsg {
                cw4_group_code_id: cw4_id,
                initial_members: initial_weights,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_code_id,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        dao_uri: None,
    };

    let addr = app
        .instantiate_contract(
            core_id,
            Addr::unchecked(CREATOR_ADDR),
            &governance_instantiate,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    // Update the block so that weights appear.
    app.update_block(|block| block.height += 1);

    addr
}
