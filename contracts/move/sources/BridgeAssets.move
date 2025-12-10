// Copyright (c) Westar Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Bridge::ETH {
    struct ETH has store {}
}

module Bridge::BTC {
    struct BTC has store {}
}

module Bridge::USDC {
    struct USDC has store {}
}

module Bridge::USDT {
    struct USDT has store {}
}

module Bridge::AssetUtil {

    const EInvalidSender: u64 = 1;

    use StarcoinFramework::BCS;
    use StarcoinFramework::Signer;
    use StarcoinFramework::Token::{Self, MintCapability, BurnCapability};

    public fun initialize<T: store>(bridge: &signer, precision: u8, ): (MintCapability<T>, BurnCapability<T>) {
        assert!(Signer::address_of(bridge) == @Bridge, EInvalidSender);
        Token::register_token<T>(bridge, precision);
        (Token::remove_mint_capability<T>(bridge), Token::remove_burn_capability<T>(bridge))
    }

    public fun token_type_name<T: store>(): vector<u8> {
        BCS::to_bytes(&Token::token_code<T>())
    }

    #[test_only]
    public fun quick_mint_for_test<T: store>(bridge: &signer, amount: u128): Token::Token<T> {
        let (mcap, bcap) = Self::initialize<T>(bridge, 9);
        let token = Token::mint_with_capability<T>(&mcap, amount);

        Token::destroy_mint_capability(mcap);
        Token::destroy_burn_capability(bcap);

        token
    }
}