#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String};

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TicketStatus {
    Valid,
    Claimable,
    Used,
    ProofNFT,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub id: u64,
    pub event_id: u64,
    pub tier_name: String,
    pub original_buyer: Address,
    pub current_owner: Address,
    pub status: TicketStatus,
    pub price: i128,
    pub is_listed_resale: bool,
    pub resale_price: i128,
    pub mint_timestamp: u64,
    pub redeem_timestamp: u64,
    pub claim_secret_hash: String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMeta {
    pub event_id: u64,
    pub organizer: Address,
    pub name: String,
    pub total_supply: u32,
    pub minted_count: u32,
    pub royalty_bps: u32, // e.g., 500 = 5%
}

#[contracttype]
pub enum DataKey {
    EventInfo,
    Ticket(u64),
    TicketCounter,
    ClaimLink(String),
}

#[contract]
pub struct EventTicketContract;

#[contractimpl]
impl EventTicketContract {
    /// Initialize Event Metadata & Royalty structure
    pub fn initialize(
        env: Env,
        organizer: Address,
        name: String,
        total_supply: u32,
        royalty_bps: u32,
    ) {
        organizer.require_auth();

        if env.storage().instance().has(&DataKey::EventInfo) {
            panic!("Contract is already initialized");
        }
        if name.len() == 0 {
            panic!("Event name cannot be empty");
        }
        if total_supply == 0 {
            panic!("Event supply must be greater than zero");
        }
        if royalty_bps > 10_000 {
            panic!("Royalty rate cannot exceed 100 percent");
        }

        let event_info = EventMeta {
            event_id: 101,
            organizer,
            name,
            total_supply,
            minted_count: 0,
            royalty_bps,
        };

        env.storage()
            .instance()
            .set(&DataKey::EventInfo, &event_info);
        env.storage().instance().set(&DataKey::TicketCounter, &0u64);
        env.events().publish(
            (symbol_short!("init"), event_info.event_id),
            (
                event_info.organizer.clone(),
                event_info.name.clone(),
                event_info.total_supply,
                event_info.royalty_bps,
            ),
        );
    }

    /// Issue a new unique ticket digital asset / claimable balance
    pub fn mint_ticket(
        env: Env,
        buyer: Address,
        tier_name: String,
        price: i128,
        claim_secret_hash: String,
    ) -> u64 {
        let mut meta: EventMeta = env.storage().instance().get(&DataKey::EventInfo).unwrap();
        if price <= 0 {
            panic!("Ticket price must be greater than zero");
        }
        if meta.minted_count >= meta.total_supply {
            panic!("Event sold out");
        }
        if claim_secret_hash.len() > 0
            && env
                .storage()
                .persistent()
                .has(&DataKey::ClaimLink(claim_secret_hash.clone()))
        {
            panic!("Claim link is already in use");
        }

        let mut counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TicketCounter)
            .unwrap_or(0);
        if counter != u64::from(meta.minted_count) {
            panic!("Ticket counter and minted inventory are inconsistent");
        }
        counter = counter
            .checked_add(1)
            .expect("Ticket counter overflow");
        meta.minted_count = meta
            .minted_count
            .checked_add(1)
            .expect("Minted inventory overflow");

        let status = if claim_secret_hash.len() > 0 {
            TicketStatus::Claimable
        } else {
            TicketStatus::Valid
        };

        let ticket = Ticket {
            id: counter,
            event_id: meta.event_id,
            tier_name,
            original_buyer: buyer.clone(),
            current_owner: buyer,
            status,
            price,
            is_listed_resale: false,
            resale_price: 0,
            mint_timestamp: env.ledger().timestamp(),
            redeem_timestamp: 0,
            claim_secret_hash: claim_secret_hash.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(counter), &ticket);
        env.storage().instance().set(&DataKey::EventInfo, &meta);
        env.storage()
            .instance()
            .set(&DataKey::TicketCounter, &counter);

        if claim_secret_hash.len() > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::ClaimLink(claim_secret_hash), &counter);
        }

        env.events().publish(
            (symbol_short!("mint"), meta.event_id),
            (
                counter,
                ticket.current_owner.clone(),
                ticket.tier_name.clone(),
                price,
            ),
        );

        counter
    }

    /// Claim ticket using unique secret link -> transfer ownership to user's wallet
    pub fn claim_ticket(env: Env, claim_secret_hash: String, new_owner: Address) -> bool {
        new_owner.require_auth();

        let ticket_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ClaimLink(claim_secret_hash.clone()))
            .expect("Invalid or expired claim link");

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .expect("Ticket not found");

        if ticket.status != TicketStatus::Claimable {
            panic!("Ticket already claimed or invalid status");
        }

        ticket.current_owner = new_owner;
        ticket.status = TicketStatus::Valid;
        ticket.claim_secret_hash = String::from_str(&env, "");

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        env.storage()
            .persistent()
            .remove(&DataKey::ClaimLink(claim_secret_hash));
        env.events().publish(
            (symbol_short!("claim"), ticket.event_id),
            (ticket_id, ticket.current_owner.clone()),
        );

        true
    }

    /// Gatekeeper verification & Check-in: validates ticket and prevents double usage
    pub fn check_in_ticket(env: Env, organizer: Address, ticket_id: u64) -> TicketStatus {
        organizer.require_auth();

        let meta: EventMeta = env.storage().instance().get(&DataKey::EventInfo).unwrap();
        if meta.organizer != organizer {
            panic!("Unauthorized gatekeeper");
        }

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .expect("Ticket not found");

        if ticket.status == TicketStatus::Used || ticket.status == TicketStatus::ProofNFT {
            panic!("DOUBLE USE PREVENTED: Ticket already redeemed!");
        }

        if ticket.status != TicketStatus::Valid {
            panic!("Ticket cannot be redeemed: Unclaimed or invalid");
        }

        // Convert ticket -> Proof of Attendance NFT
        ticket.status = TicketStatus::ProofNFT;
        ticket.redeem_timestamp = env.ledger().timestamp();
        ticket.is_listed_resale = false;
        ticket.resale_price = 0;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        env.events().publish(
            (symbol_short!("checkin"), ticket.event_id),
            (ticket_id, ticket.redeem_timestamp),
        );

        TicketStatus::ProofNFT
    }

    /// Resale listing with cap check
    pub fn list_resale(env: Env, seller: Address, ticket_id: u64, resale_price: i128) {
        seller.require_auth();

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .expect("Ticket not found");

        if ticket.current_owner != seller {
            panic!("Not ticket owner");
        }

        if ticket.status != TicketStatus::Valid {
            panic!("Only valid tickets can be listed for resale");
        }
        if ticket.is_listed_resale {
            panic!("Ticket is already listed for resale");
        }

        if ticket.price <= 0 || resale_price <= 0 {
            panic!("Ticket and resale prices must be greater than zero");
        }

        // Anti-scalping cap: Max 150% of original price.
        let max_resale = ticket
            .price
            .checked_mul(150)
            .map(|price| price / 100)
            .unwrap_or(i128::MAX);
        if resale_price > max_resale {
            panic!("Resale price exceeds anti-scalping price cap (150%)");
        }

        ticket.is_listed_resale = true;
        ticket.resale_price = resale_price;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        env.events().publish(
            (symbol_short!("listing"), ticket.event_id),
            (ticket_id, seller, resale_price),
        );
    }

    /// Transfer a listed ticket and record royalty payout values in an event
    pub fn buy_resale(env: Env, buyer: Address, ticket_id: u64) {
        buyer.require_auth();

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .expect("Ticket not found");

        if !ticket.is_listed_resale {
            panic!("Ticket is not listed for resale");
        }
        if ticket.status != TicketStatus::Valid {
            panic!("Only valid tickets can be purchased");
        }
        if ticket.current_owner == buyer {
            panic!("Ticket owner cannot purchase their own listing");
        }

        let meta: EventMeta = env.storage().instance().get(&DataKey::EventInfo).unwrap();

        // Calculate Royalty
        let royalty_bps = meta.royalty_bps as i128;
        let royalty = (ticket.resale_price / 10_000) * royalty_bps
            + ((ticket.resale_price % 10_000) * royalty_bps) / 10_000;
        let seller_payout = ticket.resale_price - royalty;

        let previous_owner = ticket.current_owner.clone();
        ticket.current_owner = buyer.clone();
        ticket.is_listed_resale = false;
        ticket.resale_price = 0;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        env.events().publish(
            (symbol_short!("resale"), ticket.event_id),
            (ticket_id, previous_owner, buyer, royalty, seller_payout),
        );
    }

    /// Fetch ticket details
    pub fn get_ticket(env: Env, ticket_id: u64) -> Ticket {
        env.storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup_event(env: &Env, total_supply: u32, royalty_bps: u32) -> (Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EventTicketContract);
        let client = EventTicketContractClient::new(env, &contract_id);
        let organizer = Address::generate(env);

        client.initialize(
            &organizer,
            &String::from_str(env, "Event"),
            &total_supply,
            &royalty_bps,
        );

        (contract_id, organizer)
    }

    #[test]
    fn initialize_rejects_invalid_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EventTicketContract);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let organizer = Address::generate(&env);

        assert!(client
            .try_initialize(&organizer, &String::from_str(&env, ""), &100, &500,)
            .is_err());
        assert!(client
            .try_initialize(&organizer, &String::from_str(&env, "Event"), &0, &500,)
            .is_err());
        assert!(client
            .try_initialize(&organizer, &String::from_str(&env, "Event"), &100, &10_001,)
            .is_err());
    }

    #[test]
    fn buy_resale_handles_large_royalty_calculation() {
        let env = Env::default();
        let (contract_id, _) = setup_event(&env, 1, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        client.mint_ticket(
            &seller,
            &String::from_str(&env, "General"),
            &i128::MAX,
            &String::from_str(&env, ""),
        );
        client.list_resale(&seller, &1, &i128::MAX);
        client.buy_resale(&buyer, &1);

        assert_eq!(client.get_ticket(&1).current_owner, buyer);
    }

    #[test]
    fn mint_rejects_duplicate_claim_links() {
        let env = Env::default();
        let (contract_id, _) = setup_event(&env, 2, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let first_buyer = Address::generate(&env);
        let second_buyer = Address::generate(&env);
        let claim_secret_hash = String::from_str(&env, "unique-claim-hash");

        client.mint_ticket(
            &first_buyer,
            &String::from_str(&env, "General"),
            &100,
            &claim_secret_hash,
        );

        assert!(client
            .try_mint_ticket(
                &second_buyer,
                &String::from_str(&env, "General"),
                &100,
                &claim_secret_hash,
            )
            .is_err());

        assert_eq!(client.get_ticket(&1).current_owner, first_buyer);
    }

    #[test]
    fn check_in_cancels_resale_listing() {
        let env = Env::default();
        let (contract_id, organizer) = setup_event(&env, 1, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let seller = Address::generate(&env);

        client.mint_ticket(
            &seller,
            &String::from_str(&env, "General"),
            &100,
            &String::from_str(&env, ""),
        );
        client.list_resale(&seller, &1, &100);
        client.check_in_ticket(&organizer, &1);

        let ticket = client.get_ticket(&1);
        assert_eq!(ticket.status, TicketStatus::ProofNFT);
        assert!(!ticket.is_listed_resale);
        assert_eq!(ticket.resale_price, 0);
        assert!(client.try_buy_resale(&Address::generate(&env), &1).is_err());
        assert_eq!(client.get_ticket(&1).current_owner, seller);
    }

    #[test]
    fn repeated_mints_preserve_unique_inventory_ids() {
        let env = Env::default();
        let (contract_id, _) = setup_event(&env, 2, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let first_buyer = Address::generate(&env);
        let second_buyer = Address::generate(&env);
        let third_buyer = Address::generate(&env);
        let tier = String::from_str(&env, "General");
        let no_claim = String::from_str(&env, "");

        assert_eq!(client.mint_ticket(&first_buyer, &tier, &100, &no_claim), 1);
        assert_eq!(client.mint_ticket(&second_buyer, &tier, &100, &no_claim), 2);
        assert!(client
            .try_mint_ticket(&third_buyer, &tier, &100, &no_claim)
            .is_err());

        assert_eq!(client.get_ticket(&1).current_owner, first_buyer);
        assert_eq!(client.get_ticket(&2).current_owner, second_buyer);
        assert!(client.try_get_ticket(&3).is_err());
    }
}
