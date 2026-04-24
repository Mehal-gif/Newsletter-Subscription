#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Env, String};

#[test]
fn test_init_and_vote() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Initialize poll with candidates
    client.init(&vec![
        &env,
        String::from_str(&env, "Alice"),
        String::from_str(&env, "Bob"),
        String::from_str(&env, "Charlie"),
    ]);

    // User1 votes for Alice
    client.vote(&user1, &String::from_str(&env, "Alice"));

    // User2 votes for Bob
    client.vote(&user2, &String::from_str(&env, "Bob"));

    // Check vote counts
    assert_eq!(client.get_votes(&String::from_str(&env, "Alice")), 1);
    assert_eq!(client.get_votes(&String::from_str(&env, "Bob")), 1);
    assert_eq!(client.get_votes(&String::from_str(&env, "Charlie")), 0);
}

#[test]
#[should_panic(expected = "already voted")]
fn test_double_vote_prevented() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    client.init(&vec![
        &env,
        String::from_str(&env, "Alice"),
        String::from_str(&env, "Bob"),
    ]);

    // Vote twice - should panic
    client.vote(&user, &String::from_str(&env, "Alice"));
    client.vote(&user, &String::from_str(&env, "Bob"));
}

#[test]
#[should_panic(expected = "Invalid candidate")]
fn test_invalid_candidate() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    client.init(&vec![&env, String::from_str(&env, "Alice")]);

    // Vote for non-existent candidate
    client.vote(&user, &String::from_str(&env, "Bob"));
}

#[test]
fn test_get_winner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    client.init(&vec![
        &env,
        String::from_str(&env, "Alice"),
        String::from_str(&env, "Bob"),
    ]);

    // 2 votes for Alice, 1 for Bob
    client.vote(&user1, &String::from_str(&env, "Alice"));
    client.vote(&user2, &String::from_str(&env, "Bob"));
    client.vote(&user3, &String::from_str(&env, "Alice"));

    let winner = client.get_winner();
    assert_eq!(winner, String::from_str(&env, "Alice"));
}
