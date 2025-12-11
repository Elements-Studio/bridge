// Copyright (c) Starcoin Contributors
// SPDX-License-Identifier: Apache-2.0

module Bridge::Committee {

    use Bridge::Crypto;
    use Bridge::EcdsaK1;
    use Bridge::Message::{Self, Blocklist, BridgeMessage};

    use StarcoinFramework::Errors;
    use StarcoinFramework::Event;
    use StarcoinFramework::Hash;
    use StarcoinFramework::Signer;
    use StarcoinFramework::SimpleMap;
    use StarcoinFramework::SimpleMap::SimpleMap;
    use StarcoinFramework::Vector;

    const ESignatureBelowThreshold: u64 = 0;
    const EDuplicatedSignature: u64 = 1;
    const EInvalidSignature: u64 = 2;
    const ENotSystemAddress: u64 = 3;
    const EValidatorBlocklistContainsUnknownKey: u64 = 4;
    const ESenderNotActiveValidator: u64 = 5;
    const EInvalidPubkeyLength: u64 = 6;
    const ECommitteeAlreadyInitiated: u64 = 7;
    const EDuplicatePubkey: u64 = 8;
    const ESenderIsNotInBridgeCommittee: u64 = 9;

    const STARCOIN_MESSAGE_PREFIX: vector<u8> = b"STARCOIN_BRIDGE_MESSAGE";

    const ECDSA_COMPRESSED_PUBKEY_LENGTH: u64 = 33;
    const ECDSA_RAW_PUBKEY_LENGTH: u64 = 64;

    //////////////////////////////////////////////////////
    // Types
    //
    struct BlocklistValidatorEvent has copy, drop, store {
        blocklisted: bool,
        public_keys: vector<vector<u8>>,
    }

    struct BridgeCommittee has store {
        // commitee pub key and weight
        // commitee pub key and weight
        members: SimpleMap<vector<u8>, CommitteeMember>,
        // Committee member registrations for the next committee creation.
        member_registrations: SimpleMap<address, CommitteeMemberRegistration>,
        // Epoch when the current committee was updated,
        // the voting power for each of the committee members are snapshot from this epoch.
        // This is mainly for verification/auditing purposes, it might not be useful for bridge operations.
        last_committee_update_epoch: u64,
    }

    struct EventHandlePod has key, store {
        committee_member_registration: Event::EventHandle<CommitteeMemberRegistration>,
        committee_update_event: Event::EventHandle<CommitteeUpdateEvent>,
        committee_member_url_update_event: Event::EventHandle<CommitteeMemberUrlUpdateEvent>,
        block_list_validator_event: Event::EventHandle<BlocklistValidatorEvent>,
    }

    struct CommitteeUpdateEvent has copy, drop, store {
        // commitee pub key and weight
        members: SimpleMap<vector<u8>, CommitteeMember>,
        stake_participation_percentage: u64,
    }

    struct CommitteeMemberUrlUpdateEvent has copy, drop, store {
        member: vector<u8>,
        new_url: vector<u8>,
    }

    struct CommitteeMember has copy, drop, store {
        /// The Sui Address of the validator
        starcoin_address: address,
        /// The public key bytes of the bridge key
        bridge_pubkey_bytes: vector<u8>,
        /// Voting power, values are voting power in the scale of 10000.
        voting_power: u64,
        /// The HTTP REST URL the member's node listens to
        /// it looks like b'https://127.0.0.1:9191'
        http_rest_url: vector<u8>,
        /// If this member is blocklisted
        blocklisted: bool,
    }

    struct CommitteeMemberRegistration has copy, drop, store {
        /// The Starcoin Address of the validator
        starcoin_address: address,
        /// The public key bytes of the bridge key
        bridge_pubkey_bytes: vector<u8>,
        /// The HTTP REST URL the member's node listens to
        /// it looks like b'https://127.0.0.1:9191'
        http_rest_url: vector<u8>,
    }

    //////////////////////////////////////////////////////
    // Public functions
    //

    public fun initialize(bridge: &signer) {
        assert!(Signer::address_of(bridge) == @Bridge, Errors::requires_address(EInvalidSignature));
        move_to(bridge, EventHandlePod {
            committee_member_registration: Event::new_event_handle<CommitteeMemberRegistration>(bridge),
            committee_update_event: Event::new_event_handle<CommitteeUpdateEvent>(bridge),
            committee_member_url_update_event: Event::new_event_handle<CommitteeMemberUrlUpdateEvent>(bridge),
            block_list_validator_event: Event::new_event_handle<BlocklistValidatorEvent>(bridge),
        })
    }


    public fun verify_signatures(
        self: &BridgeCommittee,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let (i, signature_counts) = (0, Vector::length(&signatures));
        let seen_pub_key = Vector::empty<vector<u8>>();
        let required_voting_power = Message::required_voting_power(&message);
        // add prefix to the message bytes, then hash with keccak256
        let message_bytes = STARCOIN_MESSAGE_PREFIX;
        Vector::append(&mut message_bytes, Message::serialize_message(message));
        let message_hash = Hash::keccak_256(message_bytes);

        let threshold = 0;
        while (i < signature_counts) {
            let pubkey = EcdsaK1::secp256k1_ecrecover(Vector::borrow(&signatures, i), &message_hash, 0);

            // check duplicate
            // and make sure pub key is part of the committee
            assert!(!Vector::contains(&seen_pub_key, &pubkey), Errors::invalid_state(EDuplicatedSignature));
            assert!(SimpleMap::contains_key(&self.members, &pubkey), Errors::requires_address(EInvalidSignature));

            // get committee signature weight and check pubkey is part of the committee
            let member = SimpleMap::borrow(&self.members, &pubkey);
            if (!member.blocklisted) {
                threshold = threshold + member.voting_power;
            };
            Vector::push_back(&mut seen_pub_key, pubkey);
            i = i + 1;
        };
        assert!(threshold >= required_voting_power, Errors::invalid_state(ESignatureBelowThreshold));
    }

    //////////////////////////////////////////////////////
    // Internal functions
    //

    public fun create(): BridgeCommittee {
        BridgeCommittee {
            members: SimpleMap::create<vector<u8>, CommitteeMember>(),
            member_registrations: SimpleMap::create<address, CommitteeMemberRegistration>(),
            last_committee_update_epoch: 0,
        }
    }

    public fun destroy(committee: BridgeCommittee) {
        let BridgeCommittee { members: _, member_registrations: _, last_committee_update_epoch: _ } = committee;
    }

    public fun active_validator_addresses(): vector<address> {
        // For dev/test: allow Bridge address as validator
        // In production, this should query the validator set from starcoin framework
        let validators = Vector::empty<address>();
        Vector::push_back(&mut validators, @Bridge);
        validators
    }

    public fun register(
        self: &mut BridgeCommittee,
        sender_address: address,
        bridge_pubkey_bytes: vector<u8>,
        http_rest_url: vector<u8>,
    ) acquires EventHandlePod {
        // We disallow registration after committee initiated in v1
        assert!(SimpleMap::length(&self.members) <= 0, Errors::invalid_state(ECommitteeAlreadyInitiated));

        // Ensure pubkey is valid
        assert!(
            Vector::length(&bridge_pubkey_bytes) == ECDSA_COMPRESSED_PUBKEY_LENGTH,
            Errors::invalid_state(EInvalidPubkeyLength)
        );

        // sender must be the same sender that created the validator object, this is to prevent DDoS from non-validator actor.
        // let sender = ctx.sender();
        let validators = Self::active_validator_addresses();
        assert!(Vector::contains(&validators, &sender_address), ESenderNotActiveValidator);
        // Sender is active validator, record the registration

        // In case validator need to update the info
        let registration = if (SimpleMap::contains_key(&self.member_registrations, &sender_address)) {
            let registration = SimpleMap::borrow_mut(&mut self.member_registrations, &sender_address);
            registration.http_rest_url = http_rest_url;
            registration.bridge_pubkey_bytes = bridge_pubkey_bytes;
            *registration
        } else {
            let registration = CommitteeMemberRegistration {
                starcoin_address: sender_address,
                bridge_pubkey_bytes,
                http_rest_url,
            };
            SimpleMap::add(&mut self.member_registrations, sender_address, registration);
            registration
        };

        // check uniqueness of the bridge pubkey.
        // `try_create_next_committee` will abort if bridge_pubkey_bytes are not unique and
        // that will fail the end of epoch transaction (possibly "forever", well, we
        // need to deploy proper validator changes to stop end of epoch from failing).
        Self::check_uniqueness_bridge_keys(self, bridge_pubkey_bytes);

        let event_handle_pod = borrow_global_mut<EventHandlePod>(@Bridge);
        Event::emit_event(
            &mut event_handle_pod.committee_member_registration,
            registration
        )
    }

    // This method will try to create the next committee using the registration and system state,
    // if the total stake fails to meet the minimum required percentage, it will skip the update.
    // This is to ensure we don't fail the end of epoch transaction.
    public fun try_create_next_committee(
        self: &mut BridgeCommittee,
        active_validator_voting_power: SimpleMap<address, u64>,
        min_stake_participation_percentage: u64,
        epoch: u64,
    ) acquires EventHandlePod {
        let i = 0;
        let new_members = SimpleMap::create<vector<u8>, CommitteeMember>();
        let stake_participation_percentage = 0;

        let len = SimpleMap::length(&self.member_registrations);
        while (i < len) {
            // retrieve registration
            let (_, registration) = SimpleMap::borrow_index(&self.member_registrations, i);
            // Find validator stake amount from system state

            // Process registration if it's active validator
            let voting_power = SimpleMap::borrow(&active_validator_voting_power, &registration.starcoin_address);
            stake_participation_percentage = stake_participation_percentage + *voting_power;

            let member = CommitteeMember {
                starcoin_address: registration.starcoin_address,
                bridge_pubkey_bytes: registration.bridge_pubkey_bytes,
                voting_power: *voting_power,
                http_rest_url: registration.http_rest_url,
                blocklisted: false,
            };

            // Decompress the 33-byte compressed pubkey to 65-byte uncompressed format,
            // then extract the 64-byte raw pubkey (remove 0x04 prefix) for use as map key.
            // This matches the format returned by ecdsa_recover.
            let uncompressed = EcdsaK1::decompress_pubkey(&registration.bridge_pubkey_bytes);
            let raw_pubkey = Vector::empty<u8>();
            let j = 1; // Skip 0x04 prefix
            while (j < 65) {
                Vector::push_back(&mut raw_pubkey, *Vector::borrow(&uncompressed, j));
                j = j + 1;
            };
            
            SimpleMap::add(&mut new_members, raw_pubkey, member);
            i = i + 1;
        };


        // Make sure the new committee represent enough stakes, percentage are accurate to 2DP
        if (stake_participation_percentage >= min_stake_participation_percentage) {
            // Clear registrations
            self.member_registrations = SimpleMap::create();
            // Store new committee info
            self.members = new_members;
            self.last_committee_update_epoch = epoch;

            let eh = borrow_global_mut<EventHandlePod>(@Bridge);
            Event::emit_event(&mut eh.committee_update_event, CommitteeUpdateEvent {
                members: new_members,
                stake_participation_percentage,
            })
        }
    }

    // This function applys the blocklist to the committee members, we won't need to run this very often so this is not gas optimised.
    // TODO: add tests for this function
    public fun execute_blocklist(self: &mut BridgeCommittee, blocklist: Blocklist) acquires EventHandlePod {
        let blocklisted = Message::blocklist_type(&blocklist) != 1;
        let eth_addresses = Message::blocklist_validator_addresses(&blocklist);
        let list_len = Vector::length(eth_addresses);
        let list_idx = 0;
        let member_idx = 0;
        let pub_keys = vector[];

        let members_len = SimpleMap::length(&self.members);
        while (list_idx < list_len) {
            let target_address = Vector::borrow(eth_addresses, list_idx);
            let found = false;


            while (member_idx < members_len) {
                let (pub_key, member) = SimpleMap::borrow_index_mut(&mut self.members, member_idx);
                let eth_address = Crypto::ecdsa_pub_key_to_eth_address(pub_key);

                if (*target_address == eth_address) {
                    member.blocklisted = blocklisted;
                    Vector::push_back(&mut pub_keys, *pub_key);
                    found = true;
                    member_idx = 0;
                    break
                };

                member_idx = member_idx + 1;
            };

            assert!(found, EValidatorBlocklistContainsUnknownKey);
            list_idx = list_idx + 1;
        };

        let eh = borrow_global_mut<EventHandlePod>(@Bridge);
        Event::emit_event(&mut eh.block_list_validator_event, BlocklistValidatorEvent {
            blocklisted,
            public_keys: pub_keys,
        })
    }

    public fun committee_members(self: &BridgeCommittee): &SimpleMap<vector<u8>, CommitteeMember> {
        &self.members
    }

    public fun update_node_url(
        self: &mut BridgeCommittee,
        sender_address: address,
        new_url: vector<u8>,
    ) acquires EventHandlePod {
        let eh = borrow_global_mut<EventHandlePod>(@Bridge);
        let idx = 0;
        let member_len = SimpleMap::length(&self.members);
        while (idx < member_len) {
            let (_, member) = SimpleMap::borrow_index_mut(&mut self.members, idx);
            if (member.starcoin_address == sender_address) {
                member.http_rest_url = new_url;
                Event::emit_event(
                    &mut eh.committee_member_url_update_event,
                    CommitteeMemberUrlUpdateEvent {
                        member: member.bridge_pubkey_bytes,
                        new_url,
                    });
                return
            };
            idx = idx + 1;
        };
        abort ESenderIsNotInBridgeCommittee
    }

    // Assert if `bridge_pubkey_bytes` is duplicated in `member_registrations`.
    // Dupicate keys would cause `try_create_next_committee` to fail and,
    // in consequence, an end of epoch transaction to fail (safe mode run).
    // This check will ensure the creation of the committee is correct.
    fun check_uniqueness_bridge_keys(self: &BridgeCommittee, bridge_pubkey_bytes: vector<u8>) {
        let count = SimpleMap::length(&self.member_registrations);
        // bridge_pubkey_bytes must be found once and once only
        let bridge_key_found = false;
        while (count > 0) {
            count = count - 1;
            let (_, registration) = SimpleMap::borrow_index(&self.member_registrations, count);
            if (registration.bridge_pubkey_bytes == bridge_pubkey_bytes) {
                assert!(!bridge_key_found, EDuplicatePubkey);
                bridge_key_found = true; // bridge_pubkey_bytes found, we must not have another one
            }
        };
    }

    //////////////////////////////////////////////////////
    // Test functions
    //

    #[test_only]
    public fun members(self: &BridgeCommittee): &SimpleMap<vector<u8>, CommitteeMember> {
        &self.members
    }

    #[test_only]
    public fun voting_power(member: &CommitteeMember): u64 {
        member.voting_power
    }

    #[test_only]
    public fun http_rest_url(member: &CommitteeMember): vector<u8> {
        member.http_rest_url
    }

    #[test_only]
    public fun member_registrations(
        self: &BridgeCommittee,
    ): &SimpleMap<address, CommitteeMemberRegistration> {
        &self.member_registrations
    }

    #[test_only]
    public fun blocklisted(member: &CommitteeMember): bool {
        member.blocklisted
    }

    #[test_only]
    public fun bridge_pubkey_bytes(registration: &CommitteeMemberRegistration): &vector<u8> {
        &registration.bridge_pubkey_bytes
    }

    #[test_only]
    public fun make_bridge_committee(
        members: SimpleMap<vector<u8>, CommitteeMember>,
        member_registrations: SimpleMap<address, CommitteeMemberRegistration>,
        last_committee_update_epoch: u64,
    ): BridgeCommittee {
        BridgeCommittee {
            members,
            member_registrations,
            last_committee_update_epoch,
        }
    }

    #[test_only]
    public fun make_committee_member(
        starcoin_address: address,
        bridge_pubkey_bytes: vector<u8>,
        voting_power: u64,
        http_rest_url: vector<u8>,
        blocklisted: bool,
    ): CommitteeMember {
        CommitteeMember {
            starcoin_address,
            bridge_pubkey_bytes,
            voting_power,
            http_rest_url,
            blocklisted,
        }
    }
}