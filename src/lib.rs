#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPayload {
    pub schema_version: u32,
    pub event_id: u64,
    pub ticket_id: Option<u64>,
    pub actor: Option<Address>,
    pub previous_owner: Option<Address>,
    pub new_owner: Option<Address>,
    pub status: Option<Symbol>,
    pub name: Option<String>,
    pub tier_name: Option<String>,
    pub total_supply: Option<u32>,
    pub royalty_bps: Option<u32>,
    pub price: Option<i128>,
    pub royalty: Option<i128>,
    pub seller_payout: Option<i128>,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    EventInfo,
    Ticket(u64),
    TicketCounter,
    ClaimLink(String),
    StorageVersion,
}

#[contract]
pub struct EventTicketContract;

const MAX_EVENT_NAME_BYTES: u32 = 100;
const MAX_EVENT_TICKET_SUPPLY: u32 = 1_000_000;

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
            fail(&env, ContractError::AlreadyInitialized);
        }
        if name.len() == 0 {
            fail(&env, ContractError::EmptyEventName);
        }
        if name.len() > MAX_EVENT_NAME_LENGTH {
            panic!("Event name cannot exceed 128 bytes");
        }
        if name.len() > MAX_EVENT_NAME_BYTES {
            panic!("Event name exceeds the 100-byte limit");
        }
        if total_supply == 0 {
            fail(&env, ContractError::ZeroSupply);
        }
        if total_supply > MAX_EVENT_SUPPLY {
            panic!("Event supply cannot exceed 1000000 tickets");
        }
        if total_supply > MAX_EVENT_TICKET_SUPPLY {
            panic!("Event supply exceeds the 1,000,000-ticket limit");
        }
        if royalty_bps > 10_000 {
            fail(&env, ContractError::RoyaltyAboveLimit);
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
        let mut payload = new_event_payload(&env, event_info.event_id);
        payload.actor = Some(event_info.organizer.clone());
        payload.name = Some(event_info.name.clone());
        payload.total_supply = Some(event_info.total_supply);
        payload.royalty_bps = Some(event_info.royalty_bps);
        publish_event(&env, symbol_short!("init"), payload);
    }

    /// Migrate legacy unversioned storage to the current schema version.
    pub fn migrate_storage(env: Env) -> u32 {
        let meta: EventMeta = env
            .storage()
            .instance()
            .get(&DataKey::EventInfo)
            .expect("Event is not initialized");
        meta.organizer.require_auth();

        let version: u32 = env
            .storage()
            .instance()
            .get(&DataKey::StorageVersion)
            .unwrap_or(0);
        if version == STORAGE_VERSION {
            return version;
        }
        if version != 0 {
            panic!("Unsupported storage version");
        }

        let _: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TicketCounter)
            .expect("Legacy ticket counter is missing");
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);
        STORAGE_VERSION
    }

    /// Issue a new unique ticket digital asset / claimable balance
    pub fn mint_ticket(
        env: Env,
        buyer: Address,
        tier_name: String,
        price: i128,
        claim_secret_hash: String,
    ) -> u64 {
        buyer.require_auth();
        let mut meta: EventMeta = env.storage().instance().get(&DataKey::EventInfo).unwrap();
        if price <= 0 {
            fail(&env, ContractError::InvalidTicketPrice);
        }
        if meta.minted_count >= meta.total_supply {
            fail(&env, ContractError::EventSoldOut);
        }
        if claim_secret_hash.len() > 0
            && env
                .storage()
                .persistent()
                .has(&DataKey::ClaimLink(claim_secret_hash.clone()))
        {
            fail(&env, ContractError::DuplicateClaimLink);
        }

        let mut counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TicketCounter)
            .unwrap_or(0);
        counter = counter
            .checked_add(1)
            .unwrap_or_else(|| fail(&env, ContractError::TicketCounterOverflow));
        meta.minted_count = meta
            .minted_count
            .checked_add(1)
            .unwrap_or_else(|| fail(&env, ContractError::MintedInventoryOverflow));

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

        let mut payload = new_event_payload(&env, meta.event_id);
        payload.ticket_id = Some(counter);
        payload.actor = Some(ticket.current_owner.clone());
        payload.new_owner = Some(ticket.current_owner.clone());
        payload.status = Some(ticket_status_symbol(&ticket.status));
        payload.tier_name = Some(ticket.tier_name.clone());
        payload.price = Some(price);
        publish_event(&env, symbol_short!("mint"), payload);

        counter
    }

    /// Claim ticket using unique secret link -> transfer ownership to user's wallet
    pub fn claim_ticket(env: Env, claim_secret_hash: String, new_owner: Address) -> bool {
        new_owner.require_auth();

        let ticket_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ClaimLink(claim_secret_hash.clone()))
            .unwrap_or_else(|| fail(&env, ContractError::InvalidClaimLink));

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap_or_else(|| fail(&env, ContractError::TicketNotFound));

        if ticket.status != TicketStatus::Claimable {
            fail(&env, ContractError::InvalidTicketStatus);
        }

        let previous_owner = ticket.current_owner.clone();
        ticket.current_owner = new_owner;
        ticket.status = TicketStatus::Valid;
        ticket.claim_secret_hash = String::from_str(&env, "");

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        env.storage()
            .persistent()
            .remove(&DataKey::ClaimLink(claim_secret_hash));
        let mut payload = new_event_payload(&env, ticket.event_id);
        payload.ticket_id = Some(ticket_id);
        payload.actor = Some(ticket.current_owner.clone());
        payload.previous_owner = Some(previous_owner);
        payload.new_owner = Some(ticket.current_owner.clone());
        payload.status = Some(ticket_status_symbol(&ticket.status));
        publish_event(&env, symbol_short!("claim"), payload);

        true
    }

    /// Gatekeeper verification & Check-in: validates ticket and prevents double usage
    pub fn check_in_ticket(env: Env, organizer: Address, ticket_id: u64) -> TicketStatus {
        organizer.require_auth();

        let meta: EventMeta = env
            .storage()
            .instance()
            .get(&DataKey::EventInfo)
            .unwrap_or_else(|| fail(&env, ContractError::EventNotInitialized));
        if meta.organizer != organizer {
            fail(&env, ContractError::UnauthorizedOrganizer);
        }

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap_or_else(|| fail(&env, ContractError::TicketNotFound));

        if ticket.status == TicketStatus::Used || ticket.status == TicketStatus::ProofNFT {
            fail(&env, ContractError::TicketAlreadyUsed);
        }

        if ticket.status != TicketStatus::Valid {
            fail(&env, ContractError::InvalidTicketStatus);
        }

        // Convert ticket -> Proof of Attendance NFT
        ticket.status = TicketStatus::ProofNFT;
        ticket.redeem_timestamp = env.ledger().timestamp();
        ticket.is_listed_resale = false;
        ticket.resale_price = 0;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        let mut payload = new_event_payload(&env, ticket.event_id);
        payload.ticket_id = Some(ticket_id);
        payload.actor = Some(organizer);
        payload.status = Some(ticket_status_symbol(&ticket.status));
        publish_event(&env, symbol_short!("checkin"), payload);

        TicketStatus::ProofNFT
    }

    /// Resale listing with cap check
    pub fn list_resale(env: Env, seller: Address, ticket_id: u64, resale_price: i128) {
        seller.require_auth();

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap_or_else(|| fail(&env, ContractError::TicketNotFound));

        if ticket.current_owner != seller {
            fail(&env, ContractError::NotTicketOwner);
        }

        if ticket.status != TicketStatus::Valid {
            fail(&env, ContractError::InvalidTicketStatus);
        }
        if ticket.is_listed_resale {
            fail(&env, ContractError::TicketAlreadyListed);
        }

        if ticket.price <= 0 || resale_price <= 0 {
            fail(&env, ContractError::InvalidResalePrice);
        }

        // Anti-scalping cap: Max 150% of original price.
        let max_resale = ticket
            .price
            .checked_add(ticket.price / 2)
            .unwrap_or(i128::MAX);
        if resale_price > max_resale {
            fail(&env, ContractError::ResalePriceAboveCap);
        }

        ticket.is_listed_resale = true;
        ticket.resale_price = resale_price;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        let mut payload = new_event_payload(&env, ticket.event_id);
        payload.ticket_id = Some(ticket_id);
        payload.actor = Some(seller.clone());
        payload.status = Some(ticket_status_symbol(&ticket.status));
        payload.price = Some(resale_price);
        publish_event(&env, symbol_short!("listing"), payload);
    }

    /// Transfer a listed ticket and record royalty payout values in an event
    pub fn buy_resale(env: Env, buyer: Address, ticket_id: u64) {
        buyer.require_auth();

        let mut ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap_or_else(|| fail(&env, ContractError::TicketNotFound));

        if !ticket.is_listed_resale {
            fail(&env, ContractError::TicketNotListed);
        }
        if ticket.status != TicketStatus::Valid {
            fail(&env, ContractError::InvalidTicketStatus);
        }
        if ticket.current_owner == buyer {
            fail(&env, ContractError::CannotBuyOwnListing);
        }

        let meta: EventMeta = env
            .storage()
            .instance()
            .get(&DataKey::EventInfo)
            .unwrap_or_else(|| fail(&env, ContractError::EventNotInitialized));

        // Calculate Royalty
        let resale_price = ticket.resale_price;
        let royalty_bps = meta.royalty_bps as i128;
        let royalty = (resale_price / 10_000) * royalty_bps
            + ((resale_price % 10_000) * royalty_bps) / 10_000;
        let seller_payout = resale_price - royalty;

        let previous_owner = ticket.current_owner.clone();
        ticket.current_owner = buyer.clone();
        ticket.is_listed_resale = false;
        ticket.resale_price = 0;

        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        let mut payload = new_event_payload(&env, ticket.event_id);
        payload.ticket_id = Some(ticket_id);
        payload.actor = Some(buyer.clone());
        payload.previous_owner = Some(previous_owner);
        payload.new_owner = Some(buyer);
        payload.status = Some(ticket_status_symbol(&ticket.status));
        payload.price = Some(resale_price);
        payload.royalty = Some(royalty);
        payload.seller_payout = Some(seller_payout);
        publish_event(&env, symbol_short!("resale"), payload);
    }

    /// Fetch ticket details
    pub fn get_ticket(env: Env, ticket_id: u64) -> Ticket {
        env.storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .unwrap_or_else(|| fail(&env, ContractError::TicketNotFound))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env, TryFromVal,
    };

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
    fn mint_requires_buyer_authorization() {
        let env = Env::default();
        let (contract_id, _) = setup_event(&env, 1, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let buyer = Address::generate(&env);
        env.set_auths(&[]);

        assert!(client
            .try_mint_ticket(
                &buyer,
                &String::from_str(&env, "General"),
                &100,
                &String::from_str(&env, ""),
            )
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
    fn lifecycle_events_share_versioned_payload_schema() {
        let env = Env::default();
        let (contract_id, organizer) = setup_event(&env, 1, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let claim_hash = String::from_str(&env, "event-schema-claim-hash");

        client.mint_ticket(
            &seller,
            &String::from_str(&env, "General"),
            &100,
            &claim_hash,
        );
        client.claim_ticket(&claim_hash, &seller);
        client.list_resale(&seller, &1, &100);
        client.buy_resale(&buyer, &1);
        client.check_in_ticket(&organizer, &1);

        let events = env.events().all();
        assert_eq!(events.len(), 6);
        for index in 0..events.len() {
            let (_, topics, data) = events.get(index).unwrap();
            assert_eq!(topics.len(), 2);
            assert_eq!(
                Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
                symbol_short!("event")
            );
            let payload = EventPayload::try_from_val(&env, &data).unwrap();
            assert_eq!(payload.schema_version, 1);
            assert_eq!(payload.event_id, 101);
            assert_eq!(payload.ticket_id.is_some(), index > 0);
        }
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
    fn claim_transitions_to_terminal_check_in_state() {
        let env = Env::default();
        let (contract_id, organizer) = setup_event(&env, 1, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let buyer = Address::generate(&env);
        let claim_hash = String::from_str(&env, "lifecycle-claim-hash");

        client.mint_ticket(
            &buyer,
            &String::from_str(&env, "General"),
            &100,
            &claim_hash,
        );
        assert_eq!(client.get_ticket(&1).status, TicketStatus::Claimable);

        assert!(client.claim_ticket(&claim_hash, &buyer));
        let claimed_ticket = client.get_ticket(&1);
        assert_eq!(claimed_ticket.status, TicketStatus::Valid);
        assert_eq!(claimed_ticket.claim_secret_hash, String::from_str(&env, ""));
        assert!(client.try_claim_ticket(&claim_hash, &buyer).is_err());

        assert_eq!(
            client.check_in_ticket(&organizer, &1),
            TicketStatus::ProofNFT
        );
        assert_eq!(client.get_ticket(&1).status, TicketStatus::ProofNFT);
        assert!(client.try_list_resale(&buyer, &1, &100).is_err());
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
    fn resale_cap_handles_rounding_and_large_prices() {
        let env = Env::default();
        let (contract_id, _) = setup_event(&env, 4, 500);
        let client = EventTicketContractClient::new(&env, &contract_id);
        let seller = Address::generate(&env);
        let tier = String::from_str(&env, "General");
        let no_claim = String::from_str(&env, "");

        client.mint_ticket(&seller, &tier, &101, &no_claim);
        client.list_resale(&seller, &1, &151);

        client.mint_ticket(&seller, &tier, &101, &no_claim);
        assert!(client.try_list_resale(&seller, &2, &152).is_err());

        let large_price = i128::MAX / 2;
        let exact_cap = large_price + large_price / 2;
        client.mint_ticket(&seller, &tier, &large_price, &no_claim);
        client.list_resale(&seller, &3, &exact_cap);

        client.mint_ticket(&seller, &tier, &large_price, &no_claim);
        assert!(client
            .try_list_resale(&seller, &4, &(exact_cap + 1))
            .is_err());
    }
}
