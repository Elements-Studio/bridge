module Bridge::EcdsaK1 {
    use StarcoinFramework::Errors;
    use StarcoinFramework::Option;
    use StarcoinFramework::Secp256k1;
    use StarcoinFramework::Vector;

    const ERecoverFailed: u64 = 1;
    const EInvalidSignatureLength: u64 = 2;

    /// Expected signature length: 64 bytes (r, s) + 1 byte recovery_id = 65 bytes
    const RECOVERABLE_SIGNATURE_LENGTH: u64 = 65;

    public fun decompress_pubkey(pubkey: &vector<u8>): vector<u8> {
        Secp256k1::decompress_pubkey(*pubkey)
    }

    public fun secp256k1_sign(
        private_key: &vector<u8>,
        msg: &vector<u8>,
        hash: u8,
        recoverable: bool,
    ): vector<u8> {
        Secp256k1::secp256k1_sign(private_key, msg, hash, recoverable)
    }

    /// Recovers the public key from a 65-byte recoverable signature.
    /// The signature format is: [r (32 bytes) | s (32 bytes) | recovery_id (1 byte)]
    /// The `hash` parameter is ignored; recovery_id is extracted from the signature.
    public fun secp256k1_ecrecover(signature: &vector<u8>, message: &vector<u8>, _hash: u8): vector<u8> {
        let sig_len = Vector::length(signature);
        assert!(sig_len == RECOVERABLE_SIGNATURE_LENGTH, Errors::invalid_argument(EInvalidSignatureLength));

        // Extract recovery_id from last byte
        let recovery_id = *Vector::borrow(signature, sig_len - 1);

        // Extract 64-byte signature (r, s)
        let sig_bytes = Vector::empty<u8>();
        let i = 0;
        while (i < 64) {
            Vector::push_back(&mut sig_bytes, *Vector::borrow(signature, i));
            i = i + 1;
        };

        let ecdsa_signature = Secp256k1::ecdsa_signature_from_bytes(sig_bytes);
        let raw_publickey = Secp256k1::ecdsa_recover(*message, recovery_id, &ecdsa_signature);
        assert!(Option::is_some(&raw_publickey), Errors::invalid_state(ERecoverFailed));
        Secp256k1::ecdsa_raw_public_key_to_bytes(&Option::destroy_some(raw_publickey))
    }
}
