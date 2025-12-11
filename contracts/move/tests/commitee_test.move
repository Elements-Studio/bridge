// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0
//
#[test_only]
#[allow(deprecated_usage)] // TODO: update tests to not use deprecated governance
module Bridge::CommitteeTest {
    use Bridge::ChainIDs;
    use Bridge::Committee;
    use Bridge::Committee::{
        BridgeCommittee, CommitteeMember, CommitteeMemberRegistration, execute_blocklist,
        make_bridge_committee, make_committee_member
    };
    use Bridge::Crypto;
    use Bridge::Message;
    use StarcoinFramework::SimpleMap::{Self, SimpleMap};

    // This is a token transfer message for testing
    const TEST_MSG: vector<u8> =
        x"00010a0000000000000000200000000000000000000000000000000000000000000000000000000000000064012000000000000000000000000000000000000000000000000000000000000000c8033930000000000000";

    const VALIDATOR1_PUBKEY: vector<u8> =
        x"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964";
    const VALIDATOR2_PUBKEY: vector<u8> =
        x"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62";
    const VALIDATOR3_PUBKEY: vector<u8> =
        x"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a63";

    public fun active_validator_addresses(): SimpleMap<address, u64> {
        // TDOO(VR): to create validator address
        SimpleMap::create()
    }

    public fun validator_voting_powers_for_testing(): SimpleMap<address, u64> {
        // let active_validators = Self::active_validator_addresses();
        // let voting_powers = SimpleMap::create<address, u64>();
        // // active_validators.destroy!(|validator| {
        // // let voting_power = self.validators.validator_voting_power(validator);
        // // voting_powers.insert(validator, voting_power);
        // // });
        // voting_powers
        // TODO(VR): to create validator address
        SimpleMap::create()
    }

    #[test]
    fun test_verify_signatures_good_path() {
        let committee = setup_test();
        let msg = Message::deserialize_message_test_only(TEST_MSG);
        // good path
        Committee::verify_signatures(&committee,
            msg,
            vector[
                x"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300",
                x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801",
            ],
        );
        // Clean up
        Committee::destroy(committee);
    }

    #[test, expected_failure(abort_code = Bridge::Committee::EDuplicatedSignature)]
    fun test_verify_signatures_duplicated_sig() {
        let committee = setup_test();
        let msg = Message::deserialize_message_test_only(TEST_MSG);
        // good path
        Committee::verify_signatures(&committee,
            msg,
            vector[
                x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801",
                x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801",
            ],
        );
        abort 1
    }

    #[test, expected_failure(abort_code = Bridge::Committee::EInvalidSignature)]
    fun test_verify_signatures_invalid_signature() {
        let committee = setup_test();
        let msg = Message::deserialize_message_test_only(TEST_MSG);
        // good path
        Committee::verify_signatures(&committee,
            msg,
            vector[b"6ffb3e5ce04dd138611c49520fddfbd6778879c2db4696139f53a487043409536c369c6ffaca165ce3886723cfa8b74f3e043e226e206ea25e313ea2215e6caf01"],
        );
        abort 1
    }

    #[test, expected_failure(abort_code = Bridge::Committee::ESignatureBelowThreshold)]
    fun test_verify_signatures_below_threshold() {
        let committee = setup_test();
        let msg = Message::deserialize_message_test_only(TEST_MSG);
        // good path
        Committee::verify_signatures(&committee,
            msg,
            vector[x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"],
        );
        abort 1
    }


    #[test]
    fun test_init_committee() {
        let committee = Committee::create();

        // validator registration
        Committee::register(
            &mut committee,
            @Bridge,
            VALIDATOR1_PUBKEY,
            b"",
        );
        Committee::register(
            &mut committee,
            @Bridge,
            VALIDATOR2_PUBKEY,
            b"",
        );

        // Check committee before creation
        assert!(SimpleMap::length(Committee::members(&committee)) > 0, 0);

        // let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 6000, 6000);

        assert!(2 == SimpleMap::length(Committee::members(&committee)), 1);
        let (_, member0) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        let (_, member1) = SimpleMap::borrow_index(Committee::members(&committee), 1);
        assert!(5000 == Committee::voting_power(member0), 1);
        assert!(5000 == Committee::voting_power(member1), 2);

        let members = Committee::members(&committee);
        assert!(SimpleMap::length(members) == 2, 0); // must succeed

        Committee::destroy(committee);
    }

    #[test]
    fun test_update_node_url() {
        let committee = Committee::create();

        // validator registration
        Committee::register(
            &mut committee,
            @Bridge,
            VALIDATOR1_PUBKEY,
            b"test url 1",
        );

        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 0, 6000);

        let members = Committee::members(&committee);
        assert!(SimpleMap::length(members) == 1, 0);
        let (_, member) = SimpleMap::borrow_index(members, 0);
        assert!(Committee::http_rest_url(member) == b"test url 1", 0);

        // Update URL
        Committee::update_node_url(&mut committee, @Bridge, b"test url 2");

        let members = Committee::members(&committee);
        let (_, member) = SimpleMap::borrow_index(members, 0);
        assert!(Committee::http_rest_url(member) == b"test url 2", 1);

        Committee::destroy(committee);
    }


    #[test, expected_failure(abort_code = Bridge::Committee::ESenderIsNotInBridgeCommittee)]
    fun test_update_node_url_not_validator() {
        let committee = Committee::create();

        // validator registration
        Committee::register(
            &mut committee,
            @Bridge,
            VALIDATOR1_PUBKEY,
            b"test url 1",
        );

        // let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 0, 6000);

        // Update URL should fail for validator @0xB
        Committee::update_node_url(
            &mut committee,
            @Bridge,
            b"test url",
        );

        Committee::destroy(committee);

        // test should have failed, abort
        abort 1
    }


    #[test, expected_failure(abort_code = Bridge::Committee::ESenderNotActiveValidator)]
    fun test_init_committee_not_validator() {
        let committee = Committee::create();

        // validator registration
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");

        Committee::destroy(committee);
    }

    #[test, expected_failure(abort_code = Bridge::Committee::EDuplicatePubkey)]
    fun test_init_committee_dup_pubkey() {
        let committee = Committee::create();

        // validator registration
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");

        Committee::destroy(committee);
    }

    #[test]
    fun test_init_committee_validator_become_inactive() {
        let committee = Committee::create();

        // validator registration, 3 validators registered, should have 60% voting power in total
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");
        Committee::register(&mut committee, @Bridge, VALIDATOR2_PUBKEY, b"");
        Committee::register(&mut committee, @Bridge, VALIDATOR3_PUBKEY, b"");

        // Verify validator registration
        assert!(3 == SimpleMap::length(Committee::member_registrations(&committee)), 0);

        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 0, 6000);

        assert!(SimpleMap::length(Committee::members(&committee)) > 0, 0);

        Committee::destroy(committee);
    }


    #[test]
    fun test_update_committee_registration() {
        let committee = Committee::create();

        // validator registration
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");

        // Verify registration info
        assert!(1 == SimpleMap::length(Committee::member_registrations(&committee)), 1);
        let (address, registration) = SimpleMap::borrow_index(Committee::member_registrations(&committee), 0);
        assert!(@0xA == *address, 0);
        assert!(VALIDATOR1_PUBKEY == *Committee::bridge_pubkey_bytes(registration), 0);

        // Register again with different pub key.
        Committee::register(&mut committee, @Bridge, VALIDATOR2_PUBKEY, b"");

        // Verify registration info, registration count should still be 1
        assert!(1 == SimpleMap::length(Committee::member_registrations(&committee)), 0);
        let (address, registration) = SimpleMap::borrow_index(Committee::member_registrations(&committee), 0);
        assert!(@0xA == *address, 0);
        assert!(VALIDATOR2_PUBKEY == *Committee::bridge_pubkey_bytes(registration), 0);

        Committee::destroy(committee);
    }


    #[test]
    fun test_init_committee_not_enough_stake() {
        let committee = Committee::create();

        // validator registration
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");

        // Check committee before creation
        assert!(SimpleMap::length(Committee::members(&committee)) > 0, 1);

        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 0, 6000);

        // committee should be empty because registration did not reach min stake threshold.
        assert!(SimpleMap::length(Committee::members(&committee)) > 0, 2);

        Committee::destroy(committee);
    }


    #[test, expected_failure(abort_code = Bridge::Committee::ECommitteeAlreadyInitiated)]
    fun test_register_already_initialized() {
        let committee = Committee::create();

        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");
        Committee::register(&mut committee, @Bridge, VALIDATOR2_PUBKEY, b"");

        assert!(SimpleMap::length(Committee::members(&committee)) > 0, 0);
        let voting_powers = Self::validator_voting_powers_for_testing();
        Committee::try_create_next_committee(&mut committee, voting_powers, 0, 6000);

        assert!(SimpleMap::length(Committee::members(&committee)) == 2, 1); // must succeed
        // this fails because committee is already initiated
        Committee::register(&mut committee, @Bridge, VALIDATOR1_PUBKEY, b"");

        abort 1
    }


    #[test, expected_failure(abort_code = Bridge::Committee::EInvalidPubkeyLength)]
    fun test_register_bad_pubkey() {
        let committee = Committee::create();

        Committee::register(&mut committee, @Bridge, VALIDATOR2_PUBKEY, b"");
        // this fails with invalid public key
        Committee::register(&mut committee, @Bridge, b"029bef8", b"");

        abort 1
    }

    #[test, expected_failure(abort_code = Bridge::Committee::ESignatureBelowThreshold)]
    fun test_verify_signatures_with_blocked_committee_member() {
        let committee = setup_test();
        let msg = Message::deserialize_message_test_only(TEST_MSG);

        // good path, this test should have passed in previous test
        Committee::verify_signatures(&committee,
            msg,
            vector[
                x"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300",
                x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801",
            ],
        );

        let (validator1, member) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(!Committee::blocklisted(member), 0);

        // Block a member
        let blocklist = Message::create_blocklist_message(
            ChainIDs::starcoin_testnet(),
            0,
            0, // type 0 is block
            vector[Crypto::ecdsa_pub_key_to_eth_address(validator1)],
        );
        let blocklist = Message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        let (_, blocked_member) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(Committee::blocklisted(blocked_member), 1);

        // Verify signature should fail now
        Committee::verify_signatures(
            &committee,
            msg,
            vector[
                x"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300",
                x"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801",
            ],
        );

        // Clean up
        Committee::destroy(committee);
    }

    #[test, expected_failure(abort_code = Bridge::Committee::EValidatorBlocklistContainsUnknownKey)]
    fun test_execute_blocklist_abort_upon_unknown_validator() {
        let committee = setup_test();

        // val0 and val1 are not blocked yet
        let (validator0, member0) =
            SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(!Committee::blocklisted(member0), 0);
        let (_validator1, member1) =
            SimpleMap::borrow_index(Committee::members(&committee), 1);
        assert!(!Committee::blocklisted(member1), 0);

        let eth_address0 = Crypto::ecdsa_pub_key_to_eth_address(validator0);
        let invalid_eth_address1 = x"0000000000000000000000000000000000000000";

        // Blocklist both
        let blocklist = Message::create_blocklist_message(
            ChainIDs::starcoin_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address0, invalid_eth_address1],
        );
        let blocklist = Message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // Clean up
        Committee::destroy(committee);
    }

    //
    #[test]
    fun test_execute_blocklist() {
        let committee = setup_test();

        // val0 and val1 are not blocked yet
        let (validator0, member0) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(!Committee::blocklisted(member0), 0);
        let (validator1, member1) = SimpleMap::borrow_index(Committee::members(&committee), 1);
        assert!(!Committee::blocklisted(member1), 1);

        let eth_address0 = Crypto::ecdsa_pub_key_to_eth_address(validator0);
        let eth_address1 = Crypto::ecdsa_pub_key_to_eth_address(validator1);

        // Blocklist both
        let blocklist = Message::create_blocklist_message(
            ChainIDs::starcoin_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address0, eth_address1],
        );
        let blocklist = Message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // Blocklist both reverse order
        let blocklist = Message::create_blocklist_message(
            ChainIDs::starcoin_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address1, eth_address0],
        );
        let blocklist = Message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // val 0 is blocklisted
        let (_, blocked_member) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(Committee::blocklisted(blocked_member), 1);
        // val 1 is too
        let (_, blocked_member) = SimpleMap::borrow_index(Committee::members(&committee), 1);
        assert!(Committee::blocklisted(blocked_member), 1);

        // unblocklist val1
        let blocklist = Message::create_blocklist_message(
            ChainIDs::starcoin_testnet(),
            1, // seq, this is supposed to increment, but we don't test it here
            1, // type 1 is unblocklist
            vector[eth_address1],
        );
        let blocklist = Message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // val 0 is still blocklisted
        let (_, blocked_member) = SimpleMap::borrow_index(Committee::members(&committee), 0);
        assert!(Committee::blocklisted(blocked_member), 0);
        // val 1 is not
        let (_, blocked_member) = SimpleMap::borrow_index(Committee::members(&committee), 1);
        assert!(!Committee::blocklisted(blocked_member), 0);

        // Clean up
        Committee::destroy(committee);
    }

    fun setup_test(): BridgeCommittee {
        let members = SimpleMap::create<vector<u8>, CommitteeMember>();

        let bridge_pubkey_bytes = VALIDATOR1_PUBKEY;
        SimpleMap::add(&mut members,
            bridge_pubkey_bytes,
            make_committee_member(
                @0xA,
                bridge_pubkey_bytes,
                3333,
                b"https://127.0.0.1:9191",
                false,
            ),
        );

        let bridge_pubkey_bytes = VALIDATOR2_PUBKEY;
        SimpleMap::add(
            &mut members,
            bridge_pubkey_bytes,
            make_committee_member(
                @0xC,
                bridge_pubkey_bytes,
                3333,
                b"https://127.0.0.1:9192",
                false,
            ),
        );
        make_bridge_committee(members, SimpleMap::create<address, CommitteeMemberRegistration>(), 1)
    }
}