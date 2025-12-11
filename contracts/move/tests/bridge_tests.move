// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Bridge::BridgeTests {

    use Bridge::AssetUtil;
    use Bridge::BTC::BTC;
    use Bridge::Bridge;
    use Bridge::Bridge::{new_bridge_record_for_testing, new_for_testing, transfer_status_approved,
        transfer_status_claimed, transfer_status_not_found, transfer_status_pending
    };
    use Bridge::BridgeEnv::{Self, btc_id, create_env, eth_id, test_token_id};
    use Bridge::ChainIDs;
    use Bridge::ETH::ETH;
    use Bridge::Limiter;
    use Bridge::Message;
    use Bridge::Message::to_parsed_token_transfer_message;
    use Bridge::MessageTypes;
    use Bridge::TestToken::TEST_TOKEN;

    use Bridge::Treasury;
    use StarcoinFramework::Account;
    use StarcoinFramework::BCS;
    use StarcoinFramework::Option;
    use StarcoinFramework::Signer;
    use StarcoinFramework::SimpleMap;
    use StarcoinFramework::Token;
    use StarcoinFramework::Vector;

    // common error start code for unexpected errors in tests (assertions).
    // If more than one assert in a test needs to use an unexpected error code,
    // use this as the starting error and add 1 to subsequent errors
    const UNEXPECTED_ERROR: u64 = 10293847;
    // use on tests that fail to save cleanup
    const TEST_DONE: u64 = 74839201;

    #[test]
    fun test_bridge_create() {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        let inner = Bridge::test_load_inner(BridgeEnv::bridge_ref(&bridge_wrapper));
        Bridge::assert_not_paused(inner, UNEXPECTED_ERROR);
        assert!(SimpleMap::length(Bridge::inner_token_transfer_records(inner)) == 0, 1);

        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);
        BridgeEnv::destroy_env(env);
    }


    #[test(bridge_admin = @0x1), expected_failure(abort_code = Bridge::Bridge::ENotSystemAddress)]
    fun test_bridge_create_non_system_addr(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_mainnet());
        Bridge::create_bridge_for_testing(bridge_admin, BridgeEnv::chain_id(&mut env));

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge)]
    fun test_create_bridge_default(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_custom());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge)]
    fun test_init_committee_twice(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::init_committee(&mut env, &mut bridge); // second time is a no-op

        BridgeEnv::destroy_env(env);
        Bridge::destroy_for_testing(bridge);
    }

    #[test, expected_failure(abort_code = Bridge::Bridge::ENotSystemAddress)]
    fun test_init_committee_non_system_addr() {
        let env = create_env(ChainIDs::starcoin_mainnet());
        BridgeEnv::setup_validators(&mut env, vector[
            BridgeEnv::create_validator(
                @0xA,
                100,
                b"12345678901234567890123456789012",
                b"",
                &Vector::empty<u8>()
            )
        ]);
        let bridge = BridgeEnv::create_bridge(&mut env, @0x0);
        BridgeEnv::register_committee(&mut env, &mut bridge, @0x0);
        BridgeEnv::init_committee(&mut env, &mut bridge);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Committee::ECommitteeAlreadyInitiated)]
    fun test_register_committee_after_init(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_custom());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::register_committee(&mut env, &mut bridge, @0x0);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge) ]
    fun test_register_foreign_token(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let (mint_cap, burn_cap) = AssetUtil::initialize<TEST_TOKEN>(bridge_admin, 9);
        BridgeEnv::register_foreign_token<TEST_TOKEN>(
            bridge_admin,
            &mut bridge,
            &mut env,
            mint_cap,
            burn_cap
        );
        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Treasury::ETokenSupplyNonZero)]
    fun test_register_foreign_token_non_zero_supply(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        let (mint_cap, burn_cap) = AssetUtil::initialize<TEST_TOKEN>(bridge_admin, 9);
        let coin = Token::mint_with_capability<TEST_TOKEN>(&mint_cap, 1000000000);
        BridgeEnv::register_foreign_token<TEST_TOKEN>(
            bridge_admin,
            &mut bridge,
            &mut env,
            mint_cap,
            burn_cap
        );

        Account::do_accept_token<TEST_TOKEN>(bridge_admin);
        Account::deposit(@Bridge, coin);

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);

        abort 0
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Treasury::EInvalidNotionalValue)]
    fun test_add_token_price_zero_value(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::add_tokens(
            &mut env,
            &mut bridge,
            @Bridge,
            false,
            vector[test_token_id()],
            vector[AssetUtil::token_type_name<TEST_TOKEN>()],
            vector[0],
        );

        abort 0
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EMalformedMessageError)]
    fun test_add_token_malformed_1(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::add_tokens(
            &mut env,
            &mut bridge,
            @Bridge,
            false,
            vector[test_token_id(), eth_id()],
            vector[AssetUtil::token_type_name<TEST_TOKEN>()],
            vector[10],
        );
        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
        abort 0
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EMalformedMessageError)]
    fun test_add_token_malformed_2(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::add_tokens(
            &mut env,
            &mut bridge,
            @Bridge,
            false,
            vector[test_token_id()],
            vector[
                AssetUtil::token_type_name<TEST_TOKEN>(),
                AssetUtil::token_type_name<BTC>(),
            ],
            vector[10],
        );

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
        abort 0
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EMalformedMessageError)]
    fun test_add_token_malformed_3(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::add_tokens(
            &mut env,
            &mut bridge,
            @Bridge,
            false,
            vector[test_token_id()],
            vector[AssetUtil::token_type_name<TEST_TOKEN>()],
            vector[10, 20],
        );
        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
        abort 0
    }

    #[test(bridge_admin = @Bridge)]
    fun test_add_native_token_nop(bridge_admin: &signer) {
        // adding a native token is simply a NO-OP at the moment
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::add_tokens(
            &mut env,
            &mut bridge,
            @Bridge,
            false,
            vector[test_token_id()],
            vector[AssetUtil::token_type_name<TEST_TOKEN>()],
            vector[100],
        );

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }


    #[test(bridge_admin = @Bridge)]
    fun test_execute_send_token(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let btc = BridgeEnv::get_btc(&mut env, 1);
        let eth_address = x"0000000000000000000000000000000000000000";

        BridgeEnv::send_token(&mut env, &mut bridge, @0xABCD, ChainIDs::eth_sepolia(), eth_address, btc);

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::ETokenValueIsZero)]
    fun test_execute_send_token_zero_value(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let btc = BridgeEnv::get_btc(&mut env, 0);
        let eth_address = x"0000000000000000000000000000000000000000";
        BridgeEnv::send_token(&mut env, &mut bridge, @0x0, ChainIDs::eth_sepolia(), eth_address, btc);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EInvalidEvmAddress)]
    fun test_execute_send_token_invalid_evem_address(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let btc = BridgeEnv::get_btc(&mut env, 1);
        let eth_address = x"1234";
        let validator = BridgeEnv::validators(&env);
        let val_addr = BridgeEnv::addr(Vector::borrow(validator, 0));
        BridgeEnv::send_token(&mut env, &mut bridge, val_addr, ChainIDs::eth_sepolia(), eth_address, btc);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EBridgeUnavailable)]
    fun test_execute_send_token_frozen(bridge_admin: &signer) {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        // let eth: Coin<ETH> = env.get_eth(1);
        let eth = BridgeEnv::get_eth(&mut env, 1);
        let eth_address = x"0000000000000000000000000000000000000000";
        // BridgeEnv::freeze_bridge(&mut env, &mut bridge,@0x0, UNEXPECTED_ERROR);
        BridgeEnv::freeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR);
        BridgeEnv::send_token(&mut env, &mut bridge, @0xAAAA, ChainIDs::eth_sepolia(), eth_address, eth);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EInvalidBridgeRoute)]
    fun test_execute_send_token_invalid_route(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let usdc = BridgeEnv::get_usdc(&mut env, 100);
        let eth_address = x"0000000000000000000000000000000000000000";
        BridgeEnv::send_token(&mut env, &mut bridge, @0xABCDEF, ChainIDs::eth_mainnet(), eth_address, usdc);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EUnexpectedChainID)]
    fun test_system_msg_incorrect_chain_id(bridge_admin: &signer) {
        let sender = @0x0;
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::execute_blocklist(&mut env, &mut bridge, sender, ChainIDs::starcoin_mainnet(), 0, vector[]);

        BridgeEnv::destroy_env(env);
        Bridge::destroy_for_testing(bridge);

        abort TEST_DONE
    }

    #[test]
    fun test_get_seq_num_and_increment() {
        let chain_id = ChainIDs::starcoin_testnet();
        let bridge = Bridge::new_for_testing(chain_id, @0x0);

        let inner = Bridge::test_load_inner_mut(&mut bridge);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::committee_blocklist()) == 0, 1);
        assert!(*SimpleMap::borrow(Bridge::sequence_nums(inner), &MessageTypes::committee_blocklist()) == 1, 2);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::committee_blocklist()) == 1, 3);

        let seq_nums = Bridge::sequence_nums(inner);
        // other message type nonce does not change
        assert!(!SimpleMap::contains_key(seq_nums, &MessageTypes::token()), 4);
        assert!(!SimpleMap::contains_key(seq_nums, &MessageTypes::emergency_op()), 5);
        assert!(!SimpleMap::contains_key(seq_nums, &MessageTypes::update_bridge_limit()), 6);
        assert!(!SimpleMap::contains_key(seq_nums, &MessageTypes::update_asset_price()), 7);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::token()) == 0, 8);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::emergency_op()) == 0, 9);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::update_bridge_limit()) == 0, 10);
        assert!(Bridge::test_get_current_seq_num_and_increment(inner, MessageTypes::update_asset_price()) == 0, 11);

        Bridge::destroy_for_testing(bridge);
    }

    #[test]
    fun test_update_limit() {
        let chain_id = ChainIDs::starcoin_mainnet();
        let env = create_env(chain_id);
        // let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        let inner = Bridge::test_load_inner(BridgeEnv::bridge_ref(&bridge_wrapper));

        let route = ChainIDs::get_route(ChainIDs::eth_mainnet(), ChainIDs::starcoin_mainnet());

        // Assert the starting limit is a different value
        assert!(Limiter::get_route_limit(Bridge::inner_limiter(inner), &route) != 1, 0);

        // update limit
        BridgeEnv::update_bridge_limit(
            &mut env,
            BridgeEnv::bridge_ref_mut(&mut bridge_wrapper),
            @0x0,
            ChainIDs::starcoin_mainnet(),
            ChainIDs::eth_mainnet(),
            1,
        );
        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);


        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        let inner = Bridge::test_load_inner(BridgeEnv::bridge_ref(&bridge_wrapper));

        // Assert the starting limit is a different value
        let limiter = Bridge::inner_limiter(inner);
        assert!(
            Limiter::get_route_limit(
                limiter,
                &ChainIDs::get_route(ChainIDs::eth_mainnet(), ChainIDs::starcoin_mainnet())
            ) == 1,
            1
        );
        assert!(
            Limiter::get_route_limit(
                limiter,
                &ChainIDs::get_route(ChainIDs::eth_sepolia(), ChainIDs::starcoin_testnet())
            ) == 1,
            2
        );

        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EUnexpectedChainID)]
    fun test_execute_update_bridge_limit_abort_with_unexpected_chain_id(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        // This abort because the receiving_chain (sui_mainnet) is not the same as
        // the bridge's chain_id (sui_devnet)
        BridgeEnv::update_bridge_limit(
            &mut env,
            &mut bridge,
            @0x0,
            ChainIDs::starcoin_mainnet(),
            ChainIDs::eth_mainnet(),
            1,
        );

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge)]
    fun test_update_asset_price(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        let inner = Bridge::test_load_inner_mut(&mut bridge);
        // Assert the starting limit is a different value
        assert!(Treasury::notional_value<BTC>(Bridge::inner_treasury(inner)) != 1_001_000_000, 1);
        // now change it to 100_001_000
        let msg = Message::create_update_asset_price_message(
            Treasury::token_id<BTC>(Bridge::inner_treasury(inner)),
            ChainIDs::starcoin_mainnet(),
            0,
            1_001_000_000,
        );
        let payload = Message::extract_update_asset_price(&msg);
        Bridge::test_execute_update_asset_price(inner, payload);

        // should be 1_001_000_000 now
        assert!(Treasury::notional_value<BTC>(Bridge::inner_treasury(inner)) == 1_001_000_000, 1);
        // other assets are not impacted
        assert!(Treasury::notional_value<ETH>(Bridge::inner_treasury(inner)) != 1_001_000_000, 2);

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Treasury::EInvalidNotionalValue)]
    fun test_invalid_price_update(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::update_asset_price(&mut env, &mut bridge, bridge_admin, btc_id(), 0);

        abort 0
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Treasury::EUnsupportedTokenType)]
    fun test_unsupported_token_type(bridge_admin: &signer) {
        let env = create_env(ChainIDs::starcoin_testnet());
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::update_asset_price(&mut env, &mut bridge, bridge_admin, 42, 100);

        abort 0
    }

    #[test(bridge_admin = @Bridge) ]
    fun test_execute_freeze_unfreeze(bridge_admin: &signer) {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        BridgeEnv::freeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR + 1);

        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        assert!(Bridge::inner_paused(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper))), 1);
        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);

        BridgeEnv::unfreeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR + 2);
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        assert!(!Bridge::inner_paused(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper))), 2);
        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);

        Bridge::destroy_for_testing(bridge);
        BridgeEnv::destroy_env(env);
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EBridgeNotPaused)]
    fun test_execute_unfreeze_err(bridge_admin: &signer) {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        assert!(!Bridge::inner_paused(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper))), 1);

        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);
        BridgeEnv::unfreeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR + 2);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge), expected_failure(abort_code = Bridge::Bridge::EBridgeAlreadyPaused)]
    fun test_execute_emergency_op_abort_when_already_frozen(bridge_admin: &signer) {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);

        // initially it's unfrozen
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        assert!(!Bridge::inner_paused(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper))), 1);
        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);

        // freeze it
        BridgeEnv::freeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR);

        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        assert!(Bridge::inner_paused(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper))), 1);

        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);
        // freeze it again, should abort
        BridgeEnv::freeze_bridge(&mut env, &mut bridge, @0x0, UNEXPECTED_ERROR);

        abort TEST_DONE
    }

    #[test(bridge_admin = @Bridge)]
    fun test_get_token_transfer_action_data(bridge_admin: &signer) {
        let chain_id = ChainIDs::starcoin_testnet();
        let bridge = new_for_testing(chain_id, @Bridge);
        let coin = AssetUtil::quick_mint_for_test<ETH>(bridge_admin, 123456);

        // Test when pending
        let message = Message::create_token_bridge_message(
            ChainIDs::starcoin_testnet(), // source chain
            10, // seq_num
            BCS::to_bytes(&Signer::address_of(bridge_admin)), // sender address
            ChainIDs::eth_sepolia(), // target_chain
            x"00000000000000000000000000000000000000c8", // target_address
            1u8, // token_type
            (Token::value(&coin) as u64),
        );

        let tsf_records = Bridge::inner_token_transfer_records_mut(Bridge::test_load_inner_mut(&mut bridge));
        SimpleMap::add(
            tsf_records,
            Message::key(&message),
            new_bridge_record_for_testing(message, Option::none(), false)
        );

        assert!(
            Bridge::test_get_token_transfer_action_status(&mut bridge, chain_id, 10) == transfer_status_pending(),
            0
        );
        assert!(Option::is_none(&Bridge::test_get_token_transfer_action_signatures(&mut bridge, chain_id, 10)), 1);

        // Test when ready for claim
        let message = Message::create_token_bridge_message(
            ChainIDs::starcoin_testnet(), // source chain
            11, // seq_num
            BCS::to_bytes(&Signer::address_of(bridge_admin)), // sender address
            ChainIDs::eth_sepolia(), // target_chain
            x"00000000000000000000000000000000000000c8", // target_address
            1u8, // token_type
            (Token::value(&coin) as u64),
        );

        SimpleMap::add(
            Bridge::inner_token_transfer_records_mut(Bridge::test_load_inner_mut(&mut bridge)),
            Message::key(&message),
            new_bridge_record_for_testing(message, Option::some(vector[]), false)
        );

        assert!(
            Bridge::test_get_token_transfer_action_status(&mut bridge, chain_id, 11) == transfer_status_approved(), 3
        );
        assert!(
            Bridge::test_get_token_transfer_action_signatures(&mut bridge, chain_id, 11) == Option::some(vector[]), 4
        );
        assert!(
            Bridge::test_get_parsed_token_transfer_message(&mut bridge, chain_id, 11) ==
                Option::some(
                    to_parsed_token_transfer_message(&message),
                ),
            5
        );

        // Test when already claimed
        let message = Message::create_token_bridge_message(
            ChainIDs::starcoin_testnet(), // source chain
            12, // seq_num
            BCS::to_bytes(&Signer::address_of(bridge_admin)), // sender address
            ChainIDs::eth_sepolia(), // target_chain
            x"00000000000000000000000000000000000000c8", // target_address
            1u8, // token_type
            (Token::value(&coin) as u64),
        );

        SimpleMap::add(
            Bridge::inner_token_transfer_records_mut(Bridge::test_load_inner_mut(&mut bridge)),
            Message::key(&message),
            new_bridge_record_for_testing(message, Option::some(vector[b"1234"]), false)
        );

        assert!(
            Bridge::test_get_token_transfer_action_status(&mut bridge, chain_id, 12) == transfer_status_claimed(),
            5
        );
        assert!(
            Bridge::test_get_token_transfer_action_signatures(&mut bridge, chain_id, 12) == Option::some(
                vector[b"1234"]
            ),
            6
        );
        assert!(
            Bridge::test_get_parsed_token_transfer_message(&mut bridge, chain_id, 12) == Option::some(
                to_parsed_token_transfer_message(&message),
            ), 7);

        // Test when message not found
        assert!(
            Bridge::test_get_token_transfer_action_status(&mut bridge, chain_id, 13) == transfer_status_not_found(),
            8
        );
        assert!(Bridge::test_get_token_transfer_action_signatures(&mut bridge, chain_id, 13) == Option::none(), 9);
        assert!(Bridge::test_get_parsed_token_transfer_message(&mut bridge, chain_id, 13) == Option::none(), 10);

        Bridge::destroy_for_testing(bridge);
        Treasury::burn_token<ETH>(coin)
    }

    #[test, expected_failure(abort_code = Bridge::Treasury::EUnsupportedTokenType)]
    fun test_get_metadata_no_token() {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        // let _bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);

        let treasury = Bridge::inner_treasury(Bridge::test_load_inner(BridgeEnv::bridge_ref(&mut bridge_wrapper)));
        Treasury::notional_value<TEST_TOKEN>(treasury);

        abort 0
    }

    #[test]
    fun change_url() {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        // let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0xAAAA);
        Bridge::update_node_url(BridgeEnv::bridge_ref_mut(&mut bridge_wrapper), @0xAAAA, b"<url_here>");
        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);
        BridgeEnv::destroy_env(env);
    }

    #[test, expected_failure(abort_code = Bridge::Committee::ESenderIsNotInBridgeCommittee)]
    fun change_url_bad_sender() {
        let chain_id = ChainIDs::starcoin_testnet();
        let env = create_env(chain_id);
        // let bridge = BridgeEnv::create_bridge_default(&mut env, bridge_admin);
        let bridge_wrapper = BridgeEnv::bridge_wrapper(&mut env, @0x0);
        Bridge::update_node_url(BridgeEnv::bridge_ref_mut(&mut bridge_wrapper), @0x0, b"<url_here>");

        BridgeEnv::destroy_bridge_wrapper(bridge_wrapper);

        abort 0
    }
}