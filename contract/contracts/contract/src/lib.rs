#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Map, Symbol, Vec,
};

// ─── Data Structures ──────────────────────────────────────────────────────────

/// Tiers available for newsletter subscriptions
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscriptionTier {
    Basic,
    Pro,
    Founding,
}

/// A subscriber's on-chain record
#[contracttype]
#[derive(Clone, Debug)]
pub struct Subscription {
    pub subscriber: Address,
    pub tier: SubscriptionTier,
    pub start_ledger: u32,
    pub expiry_ledger: u32,
    pub auto_renew: bool,
}

/// Contract-level configuration set by the publisher
#[contracttype]
#[derive(Clone, Debug)]
pub struct Config {
    pub publisher: Address,
    pub payment_token: Address, // XLM or any SEP-41 token
    pub basic_price: i128,      // price in stroops per period
    pub pro_price: i128,
    pub founding_price: i128,
    pub period_ledgers: u32, // ~30 days worth of ledgers (~518,400)
    pub paused: bool,
}

// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Config,
    Subscription(Address),
    SubscriberList,
    TotalRevenue,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct NewsletterSubscription;

#[contractimpl]
impl NewsletterSubscription {
    // ── Initialise ────────────────────────────────────────────────────────────

    /// Called once by the publisher to bootstrap the contract.
    pub fn initialize(
        env: Env,
        publisher: Address,
        payment_token: Address,
        basic_price: i128,
        pro_price: i128,
        founding_price: i128,
        period_ledgers: u32,
    ) {
        // Ensure this can only be called once
        if env.storage().instance().has(&DataKey::Config) {
            panic!("already initialized");
        }

        publisher.require_auth();

        let config = Config {
            publisher,
            payment_token,
            basic_price,
            pro_price,
            founding_price,
            period_ledgers,
            paused: false,
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage()
            .instance()
            .set(&DataKey::TotalRevenue, &0_i128);

        let empty: Vec<Address> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::SubscriberList, &empty);

        env.events()
            .publish((Symbol::new(&env, "initialized"),), ());
    }

    // ── Subscribe ─────────────────────────────────────────────────────────────

    /// Purchase or renew a subscription for `periods` billing periods.
    pub fn subscribe(
        env: Env,
        subscriber: Address,
        tier: SubscriptionTier,
        periods: u32,
        auto_renew: bool,
    ) {
        subscriber.require_auth();

        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        if config.paused {
            panic!("contract is paused");
        }

        if periods == 0 {
            panic!("periods must be >= 1");
        }

        let unit_price = match tier {
            SubscriptionTier::Basic => config.basic_price,
            SubscriptionTier::Pro => config.pro_price,
            SubscriptionTier::Founding => config.founding_price,
        };
        let total_cost = unit_price * periods as i128;

        // Pull payment from subscriber → contract
        let token_client = token::Client::new(&env, &config.payment_token);
        token_client.transfer(
            &subscriber,
            &env.current_contract_address(),
            &total_cost,
        );

        // Calculate expiry from now (or extend existing)
        let current_ledger = env.ledger().sequence();
        let (start, expiry) =
            if let Some(existing) = Self::get_subscription_internal(&env, &subscriber) {
                let base = if existing.expiry_ledger > current_ledger {
                    existing.expiry_ledger
                } else {
                    current_ledger
                };
                (existing.start_ledger, base + config.period_ledgers * periods)
            } else {
                (
                    current_ledger,
                    current_ledger + config.period_ledgers * periods,
                )
            };

        let sub = Subscription {
            subscriber: subscriber.clone(),
            tier: tier.clone(),
            start_ledger: start,
            expiry_ledger: expiry,
            auto_renew,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(subscriber.clone()), &sub);

        // Update subscriber list (deduplicated)
        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::SubscriberList)
            .unwrap_or(Vec::new(&env));
        if !list.contains(&subscriber) {
            list.push_back(subscriber.clone());
            env.storage()
                .instance()
                .set(&DataKey::SubscriberList, &list);
        }

        // Track revenue
        let prev_revenue: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRevenue)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalRevenue, &(prev_revenue + total_cost));

        env.events().publish(
            (Symbol::new(&env, "subscribed"), subscriber),
            (tier, periods, expiry),
        );
    }

    // ── Cancel / Upgrade ──────────────────────────────────────────────────────

    /// Subscriber opts out of auto-renewal (access remains until expiry).
    pub fn cancel_auto_renew(env: Env, subscriber: Address) {
        subscriber.require_auth();
        let key = DataKey::Subscription(subscriber.clone());
        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");
        sub.auto_renew = false;
        env.storage().persistent().set(&key, &sub);
        env.events()
            .publish((Symbol::new(&env, "auto_renew_cancelled"), subscriber), ());
    }

    /// Upgrade (or downgrade) tier — charges/credits the delta for remaining time.
    /// For simplicity this implementation requires paying the full new-tier price
    /// for 1 additional period and resets the expiry clock from now.
    pub fn change_tier(env: Env, subscriber: Address, new_tier: SubscriptionTier) {
        subscriber.require_auth();

        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        let unit_price = match new_tier.clone() {
            SubscriptionTier::Basic => config.basic_price,
            SubscriptionTier::Pro => config.pro_price,
            SubscriptionTier::Founding => config.founding_price,
        };

        let token_client = token::Client::new(&env, &config.payment_token);
        token_client.transfer(
            &subscriber,
            &env.current_contract_address(),
            &unit_price,
        );

        let key = DataKey::Subscription(subscriber.clone());
        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&key)
            .expect("no subscription found");

        let current_ledger = env.ledger().sequence();
        sub.tier = new_tier.clone();
        sub.expiry_ledger = current_ledger + config.period_ledgers;

        env.storage().persistent().set(&key, &sub);
        env.events().publish(
            (Symbol::new(&env, "tier_changed"), subscriber),
            new_tier,
        );
    }

    // ── Publisher Controls ────────────────────────────────────────────────────

    /// Withdraw accumulated revenue to the publisher wallet.
    pub fn withdraw(env: Env, amount: i128) {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        config.publisher.require_auth();

        let token_client = token::Client::new(&env, &config.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &config.publisher,
            &amount,
        );

        env.events()
            .publish((Symbol::new(&env, "withdrawal"),), amount);
    }

    /// Pause / unpause new subscriptions (emergency circuit-breaker).
    pub fn set_paused(env: Env, paused: bool) {
        let mut config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        config.publisher.require_auth();
        config.paused = paused;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    /// Update subscription prices.
    pub fn update_prices(
        env: Env,
        basic_price: i128,
        pro_price: i128,
        founding_price: i128,
    ) {
        let mut config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        config.publisher.require_auth();
        config.basic_price = basic_price;
        config.pro_price = pro_price;
        config.founding_price = founding_price;
        env.storage().instance().set(&DataKey::Config, &config);

        env.events()
            .publish((Symbol::new(&env, "prices_updated"),), ());
    }

    // ── Read-Only Views ───────────────────────────────────────────────────────

    /// Returns true if the address holds an active (non-expired) subscription.
    pub fn is_active(env: Env, subscriber: Address) -> bool {
        if let Some(sub) =
            Self::get_subscription_internal(&env, &subscriber)
        {
            sub.expiry_ledger > env.ledger().sequence()
        } else {
            false
        }
    }

    /// Full subscription record for a subscriber.
    pub fn get_subscription(env: Env, subscriber: Address) -> Option<Subscription> {
        Self::get_subscription_internal(&env, &subscriber)
    }

    /// Current contract configuration.
    pub fn get_config(env: Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized")
    }

    /// Total revenue collected (in token's smallest unit).
    pub fn total_revenue(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRevenue)
            .unwrap_or(0)
    }

    /// Number of unique addresses that have ever subscribed.
    pub fn subscriber_count(env: Env) -> u32 {
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::SubscriberList)
            .unwrap_or(Vec::new(&env));
        list.len()
    }

    // ── Internal Helpers ──────────────────────────────────────────────────────

    fn get_subscription_internal(env: &Env, subscriber: &Address) -> Option<Subscription> {
        env.storage()
            .persistent()
            .get(&DataKey::Subscription(subscriber.clone()))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Env,
    };

    fn setup() -> (Env, Address, Address, Address, NewsletterSubscriptionClient) {
        let env = Env::default();
        env.mock_all_auths();

        let publisher = Address::generate(&env);
        let subscriber = Address::generate(&env);

        // Deploy a SAC (Stellar Asset Contract) as our test token
        let token_id = env.register_stellar_asset_contract_v2(publisher.clone());
        let token_admin = StellarAssetClient::new(&env, &token_id.address());
        // Mint 10,000 XLM-equivalent to subscriber
        token_admin.mint(&subscriber, &10_000_0000000_i128);

        let contract_id = env.register(NewsletterSubscription, ());
        let client = NewsletterSubscriptionClient::new(&env, &contract_id);

        // 518,400 ledgers ≈ 30 days at 5 s/ledger
        client.initialize(
            &publisher,
            &token_id.address(),
            &100_0000000_i128, // Basic:   100 XLM/period
            &250_0000000_i128, // Pro:     250 XLM/period
            &500_0000000_i128, // Founding: 500 XLM/period
            &518_400_u32,
        );

        (env, publisher, subscriber, token_id.address(), client)
    }

    #[test]
    fn test_subscribe_basic() {
        let (env, _publisher, subscriber, _token, client) = setup();
        client.subscribe(&subscriber, &SubscriptionTier::Basic, &1, &true);
        assert!(client.is_active(&subscriber));
    }

    #[test]
    fn test_subscription_expires() {
        let (env, _publisher, subscriber, _token, client) = setup();
        client.subscribe(&subscriber, &SubscriptionTier::Basic, &1, &false);

        // Fast-forward past the expiry
        env.ledger().with_mut(|info| {
            info.sequence_number += 518_401;
        });

        assert!(!client.is_active(&subscriber));
    }

    #[test]
    fn test_renew_extends_expiry() {
        let (env, _publisher, subscriber, _token, client) = setup();
        client.subscribe(&subscriber, &SubscriptionTier::Pro, &1, &true);
        let first_expiry = client
            .get_subscription(&subscriber)
            .unwrap()
            .expiry_ledger;

        client.subscribe(&subscriber, &SubscriptionTier::Pro, &1, &true);
        let second_expiry = client
            .get_subscription(&subscriber)
            .unwrap()
            .expiry_ledger;

        assert!(second_expiry > first_expiry);
    }

    #[test]
    fn test_cancel_auto_renew() {
        let (_env, _publisher, subscriber, _token, client) = setup();
        client.subscribe(&subscriber, &SubscriptionTier::Basic, &1, &true);
        client.cancel_auto_renew(&subscriber);
        let sub = client.get_subscription(&subscriber).unwrap();
        assert!(!sub.auto_renew);
    }

    #[test]
    fn test_revenue_tracking() {
        let (_env, _publisher, subscriber, _token, client) = setup();
        client.subscribe(&subscriber, &SubscriptionTier::Founding, &2, &false);
        // 500 XLM * 2 periods = 1000 XLM in stroops
        assert_eq!(client.total_revenue(), 1_000_0000000_i128);
    }

    #[test]
    fn test_update_prices() {
        let (_env, publisher, _subscriber, _token, client) = setup();
        client.update_prices(
            &publisher,
            &50_0000000_i128,
            &150_0000000_i128,
            &300_0000000_i128,
        );
        let config = client.get_config();
        assert_eq!(config.basic_price, 50_0000000_i128);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_paused_blocks_subscribe() {
        let (_env, publisher, subscriber, _token, client) = setup();
        client.set_paused(&publisher, &true);
        client.subscribe(&subscriber, &SubscriptionTier::Basic, &1, &false);
    }
}