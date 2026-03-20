# Forward Secrecy and Key Rotation Plan

## Overview

This document outlines the forward secrecy considerations and key rotation strategy for the Kamuy Wallet MPC Core.

## Current State

### Encryption Security

Key shares are encrypted using:
- **Argon2id** for key derivation (64MB memory, 3 iterations, 4 threads)
- **ChaCha20-Poly1305** for authenticated encryption
- **OsRng** for all cryptographic randomness

### What is Protected

All sensitive data is encrypted:
- Secret share (private key material)
- Public key and public shares
- Chain code for BIP32 derivation
- Party ID and role
- Metadata (share_id, addresses, labels)

Only the salt and nonce (public parameters) remain unencrypted.

## Forward Secrecy Considerations

### Current Limitations

1. **No Built-in Key Rotation**: The current implementation does not have automatic key rotation. If a password is compromised, all past and future encrypted shares using that password are at risk.

2. **No Forward Secrecy for Shares**: Once a key share is decrypted, the plaintext exists in memory. While we use `ZeroizeOnDrop` to clear memory when the share is dropped, there is no perfect forward secrecy for the encryption itself.

3. **No Proactive Secret Sharing**: The implementation does not currently support proactive secret sharing (periodic resharing without changing the secret).

## Key Rotation Plan

### Phase 1: Manual Key Rotation (Current)

Users can manually rotate their encrypted key shares:

```rust
// 1. Decrypt the existing share
let share = decrypt_key_share(&encrypted, old_password)?;

// 2. Re-encrypt with a new password
let new_encrypted = encrypt_key_share(&share, new_password)?;

// 3. Securely delete the old encrypted share
// 4. Store the new encrypted share
```

### Phase 2: Automated Key Refresh (Planned)

Implement a key refresh protocol that allows parties to refresh their shares without changing the underlying secret:

1. **Share Refresh Protocol**:
   - Parties generate random polynomials with zero constant term
   - Shares of these polynomials are distributed
   - New shares = old shares + refresh shares
   - Public key remains unchanged

2. **Benefits**:
   - Compromised shares become useless after refresh
   - No need to move funds to a new address
   - Can be done periodically (e.g., monthly)

### Phase 3: Proactive Secret Sharing (Future)

Implement full proactive secret sharing:

1. **Periodic Resharing**: Automatically reshare keys at regular intervals
2. **Threshold Change**: Support changing the threshold (e.g., from 2-of-3 to 3-of-5)
3. **Share Renewal**: Allow individual parties to renew their shares

## Recommendations

### For Users

1. **Use Strong Passwords**: Use a password manager to generate and store strong, unique passwords
2. **Rotate Regularly**: Manually rotate key shares every 3-6 months
3. **Secure Storage**: Store encrypted shares in secure locations (hardware security modules, encrypted drives)
4. **Backup**: Keep offline backups of encrypted shares in multiple secure locations

### For Developers

1. **Implement Key Refresh**: Priority should be given to implementing the share refresh protocol
2. **Add Audit Logging**: Log all key operations (encryption, decryption, signing) for security monitoring
3. **Rate Limiting**: Implement rate limiting on decryption attempts to prevent brute force
4. **Hardware Security**: Consider integrating with hardware security modules (HSMs) or secure enclaves

## Security Best Practices

### Password Management

- Minimum password length: 16 characters
- Use a mix of uppercase, lowercase, numbers, and symbols
- Never reuse passwords across different services
- Consider using a passphrase (4-5 random words)

### Operational Security

- Never share passwords over insecure channels
- Use secure channels for DKG and signing (end-to-end encrypted)
- Verify party identities before participating in DKG or signing
- Monitor for suspicious activity (failed decryption attempts, unusual signing requests)

### Incident Response

If a password is suspected to be compromised:

1. **Immediate**: Stop all signing operations
2. **Assess**: Determine which shares may be affected
3. **Rotate**: Re-encrypt all affected shares with new passwords
4. **Refresh**: If share refresh is implemented, perform a refresh
5. **Monitor**: Watch for any unauthorized signing attempts

## References

- [Argon2 Specification](https://datatracker.ietf.org/doc/html/rfc9106)
- [ChaCha20-Poly1305](https://datatracker.ietf.org/doc/html/rfc8439)
- [Proactive Secret Sharing](https://www.cs.cornell.edu/courses/cs754/2001fa/proactive.pdf)
- [Threshold Cryptography Best Practices](https://eprint.iacr.org/2020/1057.pdf)
