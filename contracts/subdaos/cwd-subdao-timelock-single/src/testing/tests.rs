use cosmwasm_std::{
    from_binary,
    testing::{mock_env, mock_info},
    Addr, Attribute, Reply, SubMsg, SubMsgResult,
};
use neutron_bindings::bindings::msg::NeutronMsg;
use neutron_timelock::single::ExecuteMsg;

use crate::{
    contract::{execute, instantiate, query, reply},
    msg::{InstantiateMsg, ProposalListResponse, QueryMsg},
    proposal::{ProposalStatus, SingleChoiceProposal},
    state::{Config, CONFIG, DEFAULT_LIMIT, PROPOSALS},
    testing::mock_querier::MOCK_TIMELOCK_INITIALIZER,
};

use super::mock_querier::{mock_dependencies, MOCK_SUBDAO_CORE_ADDR};

#[test]
fn test_instantiate_test() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("neutron1unknownsender", &[]);
    let msg = InstantiateMsg {
        owner: Addr::unchecked("dao"),
        timelock_duration: 10,
    };
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert_eq!(
        "Generic error: Querier system error: No such contract: neutron1unknownsender",
        res.unwrap_err().to_string()
    );

    let info = mock_info(MOCK_TIMELOCK_INITIALIZER, &[]);

    let msg = InstantiateMsg {
        owner: Addr::unchecked("dao"),
        timelock_duration: 10,
    };
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    let res_ok = res.unwrap();
    let expected_attributes = vec![
        Attribute::new("action", "instantiate"),
        Attribute::new("owner", "dao"),
        Attribute::new("timelock_duration", "10"),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    let config = CONFIG.load(&deps.storage).unwrap();
    let expected_config = Config {
        owner: msg.owner,
        timelock_duration: msg.timelock_duration,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    assert_eq!(expected_config, config);

    let msg = InstantiateMsg {
        owner: Addr::unchecked("none"),
        timelock_duration: 10,
    };
    let res = instantiate(deps.as_mut(), env, info, msg.clone());
    let res_ok = res.unwrap();
    let expected_attributes = vec![
        Attribute::new("action", "instantiate"),
        Attribute::new("owner", "none"),
        Attribute::new("timelock_duration", "10"),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    let config = CONFIG.load(&deps.storage).unwrap();
    let expected_config = Config {
        owner: msg.owner,
        timelock_duration: msg.timelock_duration,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    assert_eq!(expected_config, config);
}

#[test]
fn test_execute_timelock_proposal() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("neutron1unknownsender", &[]);

    let msg = ExecuteMsg::TimelockProposal {
        proposal_id: 10,
        msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(
        "cwd_subdao_timelock_single::state::Config not found",
        res.unwrap_err().to_string()
    );

    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!("Unauthorized", res.unwrap_err().to_string());

    let info = mock_info(MOCK_SUBDAO_CORE_ADDR, &[]);
    let res_ok = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    let expected_attributes = vec![
        Attribute::new("action", "timelock_proposal"),
        Attribute::new("sender", MOCK_SUBDAO_CORE_ADDR),
        Attribute::new("proposal_id", "10"),
        Attribute::new("status", "timelocked"),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    assert_eq!(0, res_ok.messages.len());

    let expected_proposal = SingleChoiceProposal {
        id: 10,
        timelock_ts: env.block.time,
        msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
        status: ProposalStatus::Timelocked,
    };
    let prop = PROPOSALS.load(deps.as_mut().storage, 10u64).unwrap();
    assert_eq!(expected_proposal, prop);
}

#[test]
fn test_execute_proposal() {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    let info = mock_info("neutron1unknownsender", &[]);

    let msg = ExecuteMsg::ExecuteProposal { proposal_id: 10 };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(
        "cwd_subdao_timelock_single::state::Config not found",
        res.unwrap_err().to_string()
    );

    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(
        "cwd_subdao_timelock_single::proposal::SingleChoiceProposal not found",
        res.unwrap_err().to_string()
    );

    let wrong_prop_statuses = vec![
        ProposalStatus::Executed,
        ProposalStatus::ExecutionFailed,
        ProposalStatus::Overruled,
    ];
    for s in wrong_prop_statuses {
        let proposal = SingleChoiceProposal {
            id: 10,
            timelock_ts: env.block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
            status: s,
        };
        PROPOSALS
            .save(deps.as_mut().storage, proposal.id, &proposal)
            .unwrap();
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
        assert_eq!(
            format!("Wrong proposal status ({})", s),
            res.unwrap_err().to_string()
        )
    }

    let proposal = SingleChoiceProposal {
        id: 10,
        timelock_ts: env.block.time,
        msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
        status: ProposalStatus::Timelocked,
    };
    PROPOSALS
        .save(deps.as_mut().storage, proposal.id, &proposal)
        .unwrap();
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!("Proposal is timelocked", res.unwrap_err().to_string());

    env.block.time = env.block.time.plus_seconds(11);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let expected_attributes = vec![
        Attribute::new("action", "execute_proposal"),
        Attribute::new("sender", "neutron1unknownsender"),
        Attribute::new("proposal_id", "10"),
    ];
    assert_eq!(expected_attributes, res.attributes);
    assert_eq!(
        proposal
            .msgs
            .iter()
            .map(|msg| SubMsg::reply_on_error(msg.clone(), proposal.id))
            .collect::<Vec<SubMsg<NeutronMsg>>>(),
        res.messages
    );
    let updated_prop = PROPOSALS.load(deps.as_mut().storage, 10).unwrap();
    assert_eq!(ProposalStatus::Executed, updated_prop.status)
}

#[test]
fn test_overrule_proposal() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("neutron1unknownsender", &[]);

    let msg = ExecuteMsg::OverruleProposal { proposal_id: 10 };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(
        "cwd_subdao_timelock_single::state::Config not found",
        res.unwrap_err().to_string()
    );

    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!("Unauthorized", res.unwrap_err().to_string());

    let info = mock_info("owner", &[]);

    let wrong_prop_statuses = vec![
        ProposalStatus::Executed,
        ProposalStatus::ExecutionFailed,
        ProposalStatus::Overruled,
    ];
    for s in wrong_prop_statuses {
        let proposal = SingleChoiceProposal {
            id: 10,
            timelock_ts: env.block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
            status: s,
        };
        PROPOSALS
            .save(deps.as_mut().storage, proposal.id, &proposal)
            .unwrap();
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
        assert_eq!(
            format!("Wrong proposal status ({})", s),
            res.unwrap_err().to_string()
        )
    }

    let proposal = SingleChoiceProposal {
        id: 10,
        timelock_ts: env.block.time,
        msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
        status: ProposalStatus::Timelocked,
    };
    PROPOSALS
        .save(deps.as_mut().storage, proposal.id, &proposal)
        .unwrap();
    let res_ok = execute(deps.as_mut(), env, info.clone(), msg).unwrap();
    assert_eq!(0, res_ok.messages.len());
    let expected_attributes = vec![
        Attribute::new("action", "overrule_proposal"),
        Attribute::new("sender", info.sender),
        Attribute::new("proposal_id", proposal.id.to_string()),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    let updated_prop = PROPOSALS.load(deps.as_mut().storage, 10).unwrap();
    assert_eq!(ProposalStatus::Overruled, updated_prop.status);
}

#[test]
fn execute_update_config() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("neutron1unknownsender", &[]);

    let msg = ExecuteMsg::UpdateConfig {
        owner: None,
        timelock_duration: Some(20),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(
        "cwd_subdao_timelock_single::state::Config not found",
        res.unwrap_err().to_string()
    );

    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!("Unauthorized", res.unwrap_err().to_string());

    let info = mock_info("owner", &[]);
    let config = Config {
        owner: Addr::unchecked("none"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!("Unauthorized", res.unwrap_err().to_string());

    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 10,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let res_ok = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res_ok.messages.len());
    let expected_attributes = vec![
        Attribute::new("action", "update_config"),
        Attribute::new("owner", "owner"),
        Attribute::new("timelock_duration", "20"),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    let updated_config = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        updated_config,
        Config {
            owner: Addr::unchecked("owner"),
            timelock_duration: 20,
            subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR)
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("neutron1newowner".to_string()),
        timelock_duration: None,
    };

    let res_ok = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(0, res_ok.messages.len());
    let expected_attributes = vec![
        Attribute::new("action", "update_config"),
        Attribute::new("owner", "neutron1newowner"),
        Attribute::new("timelock_duration", "20"),
    ];
    assert_eq!(expected_attributes, res_ok.attributes);
    let updated_config = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        updated_config,
        Config {
            owner: Addr::unchecked("neutron1newowner"),
            timelock_duration: 20,
            subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR)
        }
    );

    // old owner
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!("Unauthorized", err.to_string());
}

#[test]
fn test_query_config() {
    let mut deps = mock_dependencies();
    let config = Config {
        owner: Addr::unchecked("owner"),
        timelock_duration: 20,
        subdao: Addr::unchecked(MOCK_SUBDAO_CORE_ADDR),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();
    let query_msg = QueryMsg::Config {};
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_config: Config = from_binary(&res).unwrap();
    assert_eq!(config, queried_config)
}

#[test]
fn test_query_proposals() {
    let mut deps = mock_dependencies();
    for i in 1..=100 {
        let prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        PROPOSALS.save(deps.as_mut().storage, i, &prop).unwrap();
    }
    for i in 1..=100 {
        let query_msg = QueryMsg::Proposal { proposal_id: i };
        let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
        let queried_prop: SingleChoiceProposal = from_binary(&res).unwrap();
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, queried_prop)
    }

    let query_msg = QueryMsg::ListProposals {
        start_after: None,
        limit: None,
    };
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_props: ProposalListResponse = from_binary(&res).unwrap();
    for (p, i) in queried_props.proposals.iter().zip(1..=DEFAULT_LIMIT) {
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, *p);
    }

    let query_msg = QueryMsg::ListProposals {
        start_after: None,
        limit: Some(100),
    };
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_props: ProposalListResponse = from_binary(&res).unwrap();
    for (p, i) in queried_props.proposals.iter().zip(1..=DEFAULT_LIMIT) {
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, *p);
    }

    let query_msg = QueryMsg::ListProposals {
        start_after: None,
        limit: Some(10),
    };
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_props: ProposalListResponse = from_binary(&res).unwrap();
    for (p, i) in queried_props.proposals.iter().zip(1..=10) {
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, *p);
    }

    let query_msg = QueryMsg::ListProposals {
        start_after: Some(50),
        limit: None,
    };
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_props: ProposalListResponse = from_binary(&res).unwrap();
    for (p, i) in queried_props.proposals.iter().zip(51..=DEFAULT_LIMIT + 50) {
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, *p);
    }

    let query_msg = QueryMsg::ListProposals {
        start_after: Some(90),
        limit: None,
    };
    let res = query(deps.as_ref(), mock_env(), query_msg).unwrap();
    let queried_props: ProposalListResponse = from_binary(&res).unwrap();
    for (p, i) in queried_props.proposals.iter().zip(91..=100) {
        let expected_prop = SingleChoiceProposal {
            id: i,
            timelock_ts: mock_env().block.time,
            msgs: vec![NeutronMsg::remove_interchain_query(i).into()],
            status: ProposalStatus::Timelocked,
        };
        assert_eq!(expected_prop, *p);
    }
}

#[test]
fn test_reply() {
    let mut deps = mock_dependencies();
    let msg = Reply {
        id: 10,
        result: SubMsgResult::Err("error".to_string()),
    };
    let err = reply(deps.as_mut(), mock_env(), msg.clone()).unwrap_err();
    assert_eq!("no such proposal (10)", err.to_string());

    let prop = SingleChoiceProposal {
        id: 10,
        timelock_ts: mock_env().block.time,
        msgs: vec![NeutronMsg::remove_interchain_query(1).into()],
        status: ProposalStatus::Timelocked,
    };
    PROPOSALS.save(deps.as_mut().storage, 10, &prop).unwrap();
    let res_ok = reply(deps.as_mut(), mock_env(), msg).unwrap();
    assert_eq!(0, res_ok.messages.len());
    let expected_attributes = vec![Attribute::new("timelocked_proposal_execution_failed", "10")];
    assert_eq!(expected_attributes, res_ok.attributes);
}