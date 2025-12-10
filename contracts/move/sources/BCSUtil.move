// Copyright (c) Starcoin Contributors
// SPDX-License-Identifier: Apache-2.0

module Bridge::BCSUtil {
    use StarcoinFramework::Debug;
    use StarcoinFramework::Errors;
    use StarcoinFramework::Vector;

    const EOutOfRange: u64 = 1;
    const ELenOutOfRange: u64 = 2;
    const ENotBool: u64 = 3;

    public fun peel_vec_length(bcs: &mut vector<u8>): u64 {
        let (total, shift, len) = (0u64, 0, 0);
        loop {
            assert!(len <= 4, ELenOutOfRange);
            let byte = Vector::pop_back(bcs);
            len = len + 1;
            total = total | (((byte & 0x7f) << shift) as u64);
            if ((byte & 0x80) == 0) break;
            shift = shift + 7;
        };
        total
    }

    public fun peel_bool(bcs: &mut vector<u8>): bool {
        let value = Self::peel_u8(bcs);
        if (value == 0) {
            false
        } else if (value == 1) {
            true
        } else {
            abort ENotBool
        }
    }


    public fun peel_u8(bcs: &mut vector<u8>): u8 {
        assert!(Vector::length(bcs) >= 1, Errors::limit_exceeded(EOutOfRange));
        Vector::pop_back(bcs)
    }

    public fun peel_u16(bcs: &mut vector<u8>): u16 {
        assert!(Vector::length(bcs) >= 2, EOutOfRange);
        let value: u16 = 0;
        let i: u8 = 0;
        while (i < 16) {
            let byte = (Vector::pop_back(bcs) as u16);
            value = value + (byte << i);
            i = i + 8;
        };
        value
    }

    /// Read `u64` value from bcs-serialized bytes.
    public fun peel_u64(bcs: &mut vector<u8>): u64 {
        assert!(Vector::length(bcs) >= 8, EOutOfRange);
        let value: u64 = 0;
        let i: u8 = 0;
        while (i < 64) {
            let byte = (Vector::pop_back(bcs) as u64);
            value = value + (byte << i);
            i = i + 8;
        };
        value
    }

    /// Read `u128` value from bcs-serialized bytes.
    public fun peel_u128(bcs: &mut vector<u8>): u128 {
        assert!(Vector::length(bcs) >= 16, EOutOfRange);

        let value: u128 = 0;
        let i: u8 = 0;
        while (i < 128) {
            let byte = (Vector::pop_back(bcs) as u128);
            value = value + (byte << i);
            i = i + 8;
        };

        value
    }

    /// Read `u256` value from bcs-serialized bytes.
    public fun peel_u256(bcs: &mut vector<u8>): u256 {
        assert!(Vector::length(bcs) >= 32, EOutOfRange);

        let value: u256 = 0;
        let i: u256 = 0;
        while (i < 256) {
            let byte = (Vector::pop_back(bcs) as u256);
            value = value + (byte << (i as u8));
            i = i + 8;
        };
        value
    }


    /// Peel a vector of `u8` (eg string) from serialized bytes.
    public fun peel_vec_u8(bcs: &mut vector<u8>): vector<u8> {
        let len = Self::peel_vec_length(bcs);
        Debug::print(&len);
        let v = vector[];
        let i = 0;
        while (i < len) {
            Vector::push_back(&mut v, Self::peel_u8(bcs));
            i = i + 1;
        };
        v
    }

    public fun peel_vec_u64(bcs: &mut vector<u8>): vector<u64> {
        let len = Self::peel_vec_length(bcs);
        let v = vector[];
        let i = 0;
        while (i < len) {
            Vector::push_back(&mut v, Self::peel_u64(bcs));
            i = i + 1;
        };
        v
    }

    /// Peel a `vector<vector<u8>>` (eg vec of string) from serialized bytes.
    public fun peel_vec_vec_u8(bcs: &mut vector<u8>): vector<vector<u8>> {
        let len = Self::peel_vec_length(bcs);

        let result = Vector::empty<vector<u8>>();
        let i = 0;
        while (i < len) {
            let inner_len = Self::peel_vec_length(bcs);
            let inner_vec = vector[];
            let j = 0;
            while (j < inner_len) {
                Vector::push_back(&mut inner_vec, Self::peel_u8(bcs));
                j = j + 1;
            };
            Vector::push_back(&mut result, inner_vec);
            i = i + 1;
        };
        result
    }

    public fun into_remainder_bytes(bcs: vector<u8>): vector<u8> {
        let result = copy bcs;
        Vector::reverse(&mut result);
        result
    }
}
