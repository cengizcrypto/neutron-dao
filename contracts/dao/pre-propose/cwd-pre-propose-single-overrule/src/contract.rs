#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult, WasmMsg,
};
use cw2::set_contract_version;

use error::PreProposeOverruleError;

use crate::error;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, ProposeMessage, ProposeMessageInternal, QueryMsg,
    TimelockExecuteMsg,
};
use cwd_pre_propose_base::{
    error::PreProposeError,
    msg::{ExecuteMsg as ExecuteBase, InstantiateMsg as InstantiateBase},
    state::PreProposeContract,
};

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

            let overrule_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: timelock_contract_addr.to_string(),
                msg: to_binary(&TimelockExecuteMsg::OverruleProposal { proposal_id })?,
                funds: vec![],
            });

            let internal_msg = ExecuteInternal::Propose {
                msg: ProposeMessageInternal::Propose {
                    // Fill in proposer based on message sender.
                    proposer: Some(info.sender.to_string()),
                    title: "Overrule proposal".to_string(),
                    description: "Reject the decision made by subdao".to_string(),
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    PrePropose::default().query(deps, env, msg)
}
