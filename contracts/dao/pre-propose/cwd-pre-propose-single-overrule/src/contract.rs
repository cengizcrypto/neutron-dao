#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    WasmMsg,
};
use cw2::set_contract_version;
use error::PreProposeOverruleError;

use crate::error;
use crate::msg::{
    DaoProposalQueryMsg, ExecuteMsg, InstantiateMsg, MainDaoQueryMsg, ProposeMessage,
    ProposeMessageInternal, QueryMsg, SubDao, TimelockConfig, TimelockExecuteMsg, TimelockQueryMsg,
};
// use crate::state::{Config, CONFIG};
use cwd_pre_propose_base::{
    error::PreProposeError,
    msg::{ExecuteMsg as ExecuteBase, InstantiateMsg as InstantiateBase},
    state::PreProposeContract,
};

use cwd_voting::pre_propose::ProposalCreationPolicy;
use neutron_subdao_core::msg::QueryMsg as SubdaoQueryMsg;
use neutron_subdao_core::types as SubdaoTypes;
use neutron_subdao_pre_propose_single::msg::QueryMsg as SubdaoPreProposeQueryMsg;
use neutron_subdao_proposal_single::msg as SubdaoProposalMsg;
use neutron_subdao_timelock_single::types as TimelockTypes;

pub(crate) const CONTRACT_NAME: &str = "crates.io:cwd-pre-propose-single-overrule";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type PrePropose = PreProposeContract<ProposeMessageInternal>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, PreProposeError> {
    // the contract has no info for instantiation so far, so it just calls the init function of base
    // deposit is set to zero because it makes no sense for overrule proposals
    // for open submission it's tbd
    let resp = PrePropose::default().instantiate(
        deps.branch(),
        env,
        info,
        InstantiateBase {
            deposit_info: None,
            open_proposal_submission: true,
        },
    )?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, PreProposeOverruleError> {
    // We don't want to expose the `proposer` field on the propose
    // message externally as that is to be set by this module. Here,
    // we transform an external message which omits that field into an
    // internal message which sets it.
    type ExecuteInternal = ExecuteBase<ProposeMessageInternal>;
    match msg {
        ExecuteMsg::Propose {
            msg:
                ProposeMessage::ProposeOverrule {
                    timelock_contract,
                    proposal_id,
                },
        } => {
            let timelock_contract_addr = deps.api.addr_validate(&timelock_contract)?;

            let subdao_address = get_subdao_from_timelock(&timelock_contract, &deps)?;

            // we need this check since the timelock contract might be an impostor
            if get_timelock_from_subdao(&subdao_address, &deps)? != timelock_contract {
                return Err(PreProposeOverruleError::SubdaoMisconfured {});
            }

            if !check_if_subdao_legit(&subdao_address, &deps)? {
                return Err(PreProposeOverruleError::ForbiddenSubdao {});
            }

            if !check_is_proposal_timelocked(
                &Addr::unchecked(timelock_contract_addr.clone()),
                proposal_id,
                &deps,
            )? {
                return Err(PreProposeOverruleError::ProposalWrongState {});
            }

            let overrule_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: timelock_contract_addr.to_string(),
                msg: to_binary(&TimelockExecuteMsg::OverruleProposal { proposal_id })?,
                funds: vec![],
            });

            let subdao_name = get_subdao_name(&subdao_address, &deps)?;
            let prop_desc: String =
                format!("Reject the decision made by the {} subdao", subdao_name);
            let prop_name: String = format!("Overrule proposal {} of {}", proposal_id, subdao_name);

            let internal_msg = ExecuteInternal::Propose {
                msg: ProposeMessageInternal::Propose {
                    // Fill in proposer based on message sender.
                    proposer: Some(info.sender.to_string()),
                    title: prop_name,
                    description: prop_desc,
                    msgs: vec![overrule_msg],
                },
            };

            PrePropose::default()
                .execute(deps, env, info, internal_msg)
                .map_err(|e| PreProposeOverruleError::PreProposeBase(e))
        }
        _ => Err(PreProposeOverruleError::MessageUnsupported {}),
    }
}

fn get_subdao_from_timelock(
    timelock_contract: &String,
    deps: &DepsMut,
) -> Result<Addr, PreProposeOverruleError> {
    let timelock_config: TimelockConfig = deps
        .querier
        .query_wasm_smart(timelock_contract.to_string(), &TimelockQueryMsg::Config {})?;
    Ok(timelock_config.subdao)
}

fn get_timelock_from_subdao(
    subdao_core: &Addr,
    deps: &DepsMut,
) -> Result<Addr, PreProposeOverruleError> {
    let proposal_modules: Vec<SubdaoTypes::ProposalModule> = deps.querier.query_wasm_smart(
        subdao_core,
        &SubdaoQueryMsg::ProposalModules {
            start_after: None,
            limit: Some(1),
        },
    )?;

    if proposal_modules.is_empty() {
        return Err(PreProposeOverruleError::SubdaoMisconfured {});
    }

    let prop_policy: ProposalCreationPolicy = deps.querier.query_wasm_smart(
        proposal_modules.first().unwrap().address.clone(),
        &SubdaoProposalMsg::QueryMsg::ProposalCreationPolicy {},
    )?;
    match prop_policy {
        ProposalCreationPolicy::Anyone {} => Err(PreProposeOverruleError::SubdaoMisconfured {}),
        ProposalCreationPolicy::Module { addr } => {
            let timelock: Addr = deps
                .querier
                .query_wasm_smart(addr, &SubdaoPreProposeQueryMsg::TimelockAddress {})?;
            Ok(timelock)
        }
    }
}

fn check_if_subdao_legit(
    subdao_core: &Addr,
    deps: &DepsMut,
) -> Result<bool, PreProposeOverruleError> {
    let main_dao = get_main_dao_address(&deps)?;

    let mut start_after: Option<&SubDao> = None;
    let query_limit = 10;

    loop {
        let subdao_list: Vec<SubDao> = deps.querier.query_wasm_smart(
            main_dao.clone(),
            &MainDaoQueryMsg::ListSubDaos {
                start_after: match start_after {
                    None => None,
                    Some(a) => Some(a.clone().addr),
                },
                limit: Some(query_limit),
            },
        )?;

        if subdao_list.is_empty() {
            return Ok(false);
        }

        // start_after = subdao_list.last();

        if subdao_list
            .into_iter()
            .find(|x1| x1.addr == subdao_core.clone().into_string())
            .is_some()
        {
            return Ok(true);
        };

        // if subdao_list.len() < query_limit as usize {
        //     return Ok(false)
        // }
    }
}

fn get_main_dao_address(deps: &DepsMut) -> Result<Addr, PreProposeOverruleError> {
    let dao: Addr = deps.querier.query_wasm_smart(
        PrePropose::default().proposal_module.load(deps.storage)?,
        &DaoProposalQueryMsg::Dao {},
    )?;
    Ok(dao)
}

fn check_is_proposal_timelocked(
    timelock: &Addr,
    proposal_id: u64,
    deps: &DepsMut,
) -> Result<bool, PreProposeOverruleError> {
    let proposal: TimelockTypes::SingleChoiceProposal = deps
        .querier
        .query_wasm_smart(timelock, &TimelockQueryMsg::Proposal { proposal_id })?;
    return Ok(proposal.status == TimelockTypes::ProposalStatus::Timelocked);
}

fn get_subdao_name(subdao: &Addr, deps: &DepsMut) -> Result<String, PreProposeOverruleError> {
    let subdao_config: SubdaoTypes::Config = deps
        .querier
        .query_wasm_smart(subdao, &SubdaoQueryMsg::Config {})?;
    Ok(subdao_config.name)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    PrePropose::default().query(deps, env, msg)
}
