# EventLink Soroban Contract

Rust/Soroban contract source for the EventLink ticketing prototype. This repository contains the contract crate, contract-specific CI, and the Testnet deployment helper. The web application and API live in separate repositories.

> **Network and maturity:** The deployment helper targets Stellar Testnet. The crate currently has no unit-test cases; the `cargo test` CI step verifies compilation only. Treat this code as a prototype until contract behavior, authorization, and upgrade policy have been independently reviewed.

## Repositories

| Project | Repository |
| --- | --- |
| Frontend | [EVENT_LINK](https://github.com/orbit-flow-labs/EVENT_LINK) |
| Backend API | [EVENT_LINK_BACKEND-](https://github.com/orbit-flow-labs/EVENT_LINK_BACKEND-) |
| Soroban contract | [EVENT_LINK_CONTRACT](https://github.com/orbit-flow-labs/EVENT_LINK_CONTRACT) |

## Contract interface

The current contract stores one event (`event_id` is initialized to `101`) and ticket records in Soroban storage.

| Method | Purpose |
| --- | --- |
| `initialize(organizer, name, total_supply, royalty_bps)` | Initialize the event once; rejects an empty name, zero supply, and royalties above 100%. |
| `mint_ticket(buyer, tier_name, price, claim_secret_hash)` | Mint a ticket record while inventory remains; requires a positive price. |
| `claim_ticket(claim_secret_hash, new_owner)` | Redeem a claim link and transfer the ticket record to the authenticated owner. |
| `check_in_ticket(organizer, ticket_id)` | Authorize the organizer and convert an unused, claimed ticket to `ProofNFT`. |
| `list_resale(seller, ticket_id, resale_price)` | Require the current owner and a valid ticket; enforce a positive price and 150% cap. |
| `cancel_resale(seller, ticket_id)` | Require the current owner and clear an active resale listing. |
| `buy_resale(buyer, ticket_id)` | Change ticket ownership for a listed ticket and record the royalty/seller payout values in an event. |
| `get_ticket(ticket_id)` | Read a stored ticket record. |

Lifecycle events are emitted for initialization, minting, claims, check-in, listings, and resale. An indexer can consume those events to build a history, subject to ledger event retention and indexing policy.

### Current limitations

- Only one event is represented by the contract state; per-event isolation and multi-event storage are not implemented.
- `buy_resale` updates contract ownership and calculates payout values, but does not transfer payment or distribute royalties.
- Unit tests cover large-value resale royalty calculations, claim-link uniqueness, and resale cancellation on check-in. Expand coverage for authorization, inventory, resale caps, and event payloads before relying on this contract.
- This repository does not define a contract upgrade or migration policy.

## Requirements

- Rust 1.86.0 with `rustfmt`
- The `wasm32-unknown-unknown` target
- Stellar CLI for Testnet deployment

Install the target with:

```sh
rustup toolchain install 1.86.0 --component rustfmt
rustup target add wasm32-unknown-unknown --toolchain 1.86.0
```

## Build and validate

Run from the repository root:

```sh
cargo fmt -- --check
cargo test
cargo build --target wasm32-unknown-unknown --release
```

The GitHub Actions workflow runs these same checks on pull requests and pushes to `main`.

## Deploy to Stellar Testnet

First install and configure Stellar CLI, then create or select a funded Testnet key named `eventlink_deployer`:

```sh
stellar keys generate eventlink_deployer --network testnet --fund
./scripts/deploy.sh
```

The script builds the WASM, deploys it, initializes event state, and prints `SOROBAN_CONTRACT_ID`. Supply optional environment values to customize initialization:

```sh
CONTRACT_NAME="EventLink Demo" TOTAL_SUPPLY=1000 ROYALTY_BPS=500 ./scripts/deploy.sh
```

Store the resulting contract ID in the backend's `SOROBAN_CONTRACT_ID` environment variable. Never commit deployer secrets or production credentials.

## Contributing

1. Open or select an issue and describe the invariant or behavior being changed.
2. Add Soroban tests for successful and rejected calls; avoid relying on compilation alone.
3. Run formatting, tests, and the WASM build locally.
4. Include the issue or Drips Wave task reference, validation output, and any deployment impact in the PR.
5. Coordinate interface changes with the backend and frontend maintainers.

Follow the current Drips Wave 10 contribution rules where applicable; this repository does not imply program sponsorship or acceptance.

## License

MIT. See [LICENSE](LICENSE).