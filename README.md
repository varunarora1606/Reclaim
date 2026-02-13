# Reclaim

> A non-custodial inheritance and escrow protocol on Solana with on-chain inactivity enforcement and full liquidity.

Reclaim is a Solana program that allows users to deposit SOL into program-controlled vaults while retaining full liquidity through 1:1 SOL-backed tokens. Inheritance is enforced entirely on-chain using inactivity-based logic, without custodians, admin keys, freeze authority, or off-chain oracles.

---

## Overview

Reclaim is designed as a minimal, protocol-level primitive for long-term custody and inheritance on Solana.

Core principles:
- SOL custody via PDA vaults (no private keys)
- Liquidity via mintable and burnable SPL tokens
- Ownership and inheritance enforced directly in on-chain state
- Deterministic inactivity-based inheritance logic

---

## Core Concepts

### Global State
A singleton account that stores:
- SOL vault PDA
- SPL token mint
- Total shares (token supply)

### Escrow Vault
A per-user escrow account that stores:
- Owner
- Beneficiary
- Deposited share balance
- Inactivity configuration
- Vault lifecycle state

### SOL Vault (PDA)
- Holds all deposited SOL
- Has no private key
- Can only be accessed via program instructions

### SOL-Backed Token
- 1 token = 1 lamport (share)
- Minted on deposit
- Burned on redemption or inheritance claim
- Freely transferable and tradable

---

## Vault Lifecycle
Active → Claimable → Finished

- **Active**: Owner can deposit, redeem, or check in
- **Claimable**: Inactivity period has passed
- **Finished**: Inheritance claimed and vault finalized

---

## How It Works

### 1. Initialize Global State
Initializes the global protocol accounts, including the SOL vault PDA and token mint. This is executed once for the protocol.

### 2. Create Escrow
The owner creates an escrow vault and defines a beneficiary and inactivity period. The vault starts in the `Active` state.

### 3. Deposit SOL
SOL is transferred from the owner to the SOL vault PDA and equivalent SOL-backed tokens are minted to the owner. Tokens remain fully liquid.

### 4. Check-In (Prove Activity)
The owner submits a `check_in` transaction, updating the `last_check_in` timestamp and resetting the inactivity timer.

### 5. Redeem Tokens
The token holder burns SOL-backed tokens and receives equivalent SOL from the SOL vault PDA. Redemption is permissionless while the vault is active.

### 6. Claim Inheritance
If the inactivity period has passed, the beneficiary calls `claim_inheritance`. Remaining tokens are burned, remaining SOL is transferred to the beneficiary, and the vault is marked as `Finished`.

---

## On-Chain Accounts

### GlobalState

```rust
pub struct GlobalState {
    pub sol_vault: Pubkey,
    pub token_mint: Pubkey,
    pub total_shares: u64,
    pub bump: u8,
}
```

### EscrowVault

```rust
pub struct EscrowVault {
    pub owner: Pubkey,
    pub beneficiary: Pubkey,
    pub shares: u64,
    pub last_check_in: i64,
    pub inactivity_period: i64,
    pub status: VaultStatus,
    pub bump: u8,
}
```

## Security Model

- No admin keys
- No freeze authority
- No custodial control
- No off-chain oracles
- PDA vault is the single source of truth for SOL
- Tokens are burned before SOL is released
- All state transitions are deterministic and enforced on-chain

## Invariants

- SOL vault balance always backs total token supply
- Tokens cannot be minted without SOL deposit
- Tokens cannot be redeemed after vault is finished
- Only the beneficiary can claim inheritance
- Finished vaults cannot be reused
