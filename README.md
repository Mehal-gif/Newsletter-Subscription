# 📰 NewsletterSubscription — Soroban Smart Contract

> On-chain subscription payments for independent newsletters, built on the Stellar network with Soroban smart contracts.

---

## Project Description

**NewsletterSubscription** is a trustless, non-custodial subscription billing engine for independent publishers. Instead of routing reader payments through a payment processor that takes a cut, charges chargebacks, or can de-platform you overnight, this contract lets a newsletter publisher accept recurring payments directly on the Stellar blockchain.

Readers pay in XLM (or any SEP-41-compatible token), the contract enforces access windows on-chain, and the publisher withdraws earnings at any time — no intermediary, no credit card rails, no platform risk.

Built with [Soroban](https://soroban.stellar.org/), Stellar's native smart contract platform, the contract compiles to a tiny WASM binary and runs at near-zero transaction cost (~0.00001 XLM per operation).

---

## What It Does

### For Readers (Subscribers)
1. A reader calls `subscribe(address, tier, periods, auto_renew)` and pays in tokens.
2. The contract records their subscription with an **expiry ledger** (Stellar's deterministic block clock, ~5 s per ledger, so 518,400 ledgers ≈ 30 days).
3. Any gated content delivery system (newsletter app, website, API gateway) calls `is_active(address)` to gate access — one read-only RPC call, no database needed.
4. The reader can renew at any time; the expiry simply extends forward.
5. They can cancel auto-renewal or upgrade/downgrade their tier whenever they like.

### For Publishers
1. Deploy and call `initialize(...)` once with pricing and token address.
2. Collect revenue passively — it accumulates inside the contract.
3. Call `withdraw(amount)` to pull funds to your wallet at any time.
4. Adjust prices with `update_prices(...)` without re-deploying.
5. Pause new subscriptions instantly if needed (`set_paused`).

---

## Features

### 🏷️ Three Subscription Tiers
| Tier | Default Price | Intended For |
|------|--------------|--------------|
| **Basic** | 100 XLM / period | Free-tier graduates, casual readers |
| **Pro** | 250 XLM / period | Core supporters, full archive access |
| **Founding** | 500 XLM / period | Power supporters, early believers |

Prices are fully configurable post-deploy by the publisher.

### ⏱️ Ledger-Based Expiry
Access windows are enforced by Stellar's ledger sequence number — a monotonically increasing, manipulation-resistant clock. No off-chain cron jobs or renewal daemons required.

### 🔄 Flexible Renewals & Multi-Period Purchase
Subscribers can buy multiple periods in a single transaction (e.g., a full year at once). Renewing before expiry stacks on top of the existing window, so readers never lose days.

### 🔀 Tier Upgrades & Downgrades
The `change_tier` function lets a subscriber switch tiers at any time. The contract charges one period of the new tier price and resets the expiry from the current ledger.

### 💸 Non-Custodial Revenue Withdrawal
All payments sit inside the contract's own account. The publisher calls `withdraw(amount)` and funds land directly in their Stellar wallet — no platform holding your money.

### 📊 On-Chain Analytics
- `subscriber_count()` — total unique subscribers ever
- `total_revenue()` — lifetime revenue in token stroops
- `get_subscription(address)` — full record: tier, start, expiry, auto-renew flag

### 🛑 Emergency Circuit-Breaker
`set_paused(true)` halts new subscriptions instantly while leaving existing subscribers unaffected. Useful during a contract upgrade migration.

### 🪙 Any SEP-41 Token
The payment token is set at initialization. Use native XLM, USDC on Stellar, or any custom asset — the contract is token-agnostic.

### ✅ Full Test Suite
Seven unit tests covering:
- Happy-path subscription
- Expiry enforcement
- Renewal stacking
- Auto-renew cancellation
- Revenue accounting
- Price updates
- Pause guard

---

## Project Structure

```
newsletter-subscription/
├── Cargo.toml          # Workspace & dependency config
└── src/
    └── lib.rs          # Contract logic + tests
```

---

## Quick Start

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli): `cargo install stellar-cli --locked`

### Build

```bash
stellar contract build
# Output: target/wasm32-unknown-unknown/release/newsletter_subscription.wasm
```

### Test

```bash
cargo test --features testutils
```

### Deploy to Testnet

```bash
# 1. Create a funded testnet account
stellar keys generate publisher --network testnet --fund

# 2. Deploy the contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/newsletter_subscription.wasm \
  --source publisher \
  --network testnet

# 3. Initialize (replace CONTRACT_ID, TOKEN_ADDRESS with real values)
stellar contract invoke \
  --id CONTRACT_ID \
  --source publisher \
  --network testnet \
  -- initialize \
  --publisher $(stellar keys address publisher) \
  --payment_token TOKEN_ADDRESS \
  --basic_price 1000000000 \
  --pro_price 2500000000 \
  --founding_price 5000000000 \
  --period_ledgers 518400
```

### Subscribe (Reader)

```bash
stellar contract invoke \
  --id CONTRACT_ID \
  --source reader_wallet \
  --network testnet \
  -- subscribe \
  --subscriber READER_ADDRESS \
  --tier '"Basic"' \
  --periods 1 \
  --auto_renew true
```

### Check Access

```bash
stellar contract invoke \
  --id CONTRACT_ID \
  --network testnet \
  -- is_active \
  --subscriber READER_ADDRESS
```

---

## Contract Interface (Summary)

| Function | Who Calls | Description |
|---|---|---|
| `initialize` | Publisher (once) | Bootstrap contract with prices & token |
| `subscribe` | Subscriber | Pay for N periods of a tier |
| `cancel_auto_renew` | Subscriber | Opt out of renewal flag |
| `change_tier` | Subscriber | Switch tier, pays 1 new period |
| `withdraw` | Publisher | Pull revenue to publisher wallet |
| `set_paused` | Publisher | Halt / resume new subscriptions |
| `update_prices` | Publisher | Adjust tier prices |
| `is_active` | Anyone (read) | Returns bool — access gate |
| `get_subscription` | Anyone (read) | Full subscription record |
| `get_config` | Anyone (read) | Contract configuration |
| `total_revenue` | Anyone (read) | Lifetime revenue collected |
| `subscriber_count` | Anyone (read) | Unique subscriber count |

---

## Events Emitted

| Event Topic | Data | Trigger |
|---|---|---|
| `initialized` | — | Contract first setup |
| `subscribed` | `(tier, periods, expiry_ledger)` | New or renewed subscription |
| `auto_renew_cancelled` | — | Subscriber cancels renewal |
| `tier_changed` | `new_tier` | Tier upgrade/downgrade |
| `withdrawal` | `amount` | Publisher withdraws funds |
| `prices_updated` | — | Publisher updates pricing |

---

## License

MIT — build your own newsletter economy.
wallet address: GAVEV37OJ6KTPDJTDIEOM5D45CPE2W5EF3H7U3JRGWKM3B2J7TZISG62

contract address: CBPIKGKTGRECTJUEJHWFWIHI76RBARYQ5PEMKM2UCHTF4ZD6MK6NEA3J

https://stellar.expert/explorer/testnet/contract/CBPIKGKTGRECTJUEJHWFWIHI76RBARYQ5PEMKM2UCHTF4ZD6MK6NEA3J

<img width="1920" height="1200" alt="image" src="https://github.com/user-attachments/assets/a3a415f4-403c-43ed-93bc-73f88a89e762" />
