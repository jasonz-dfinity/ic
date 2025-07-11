use super::*;
use crate::{
    neuron::{DissolveStateAndAge, NeuronBuilder},
    pb::v1::{KnownNeuronData, NeuronType},
};
use ic_base_types::PrincipalId;
use ic_nervous_system_common::{E8, ONE_DAY_SECONDS, ONE_YEAR_SECONDS};
use ic_nns_common::pb::v1::NeuronId;
use icp_ledger::Subaccount;
use maplit::{btreemap, hashmap};
use pretty_assertions::assert_eq;
use std::{collections::BTreeMap, str::FromStr};

#[test]
fn test_compute_metrics() {
    let mut neuron_store = NeuronStore::new(BTreeMap::new());
    let now = neuron_store.now();

    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                1,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: 1,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(100_000_000)
            .with_neuron_type(Some(NeuronType::Seed as i32))
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                2,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: ONE_YEAR_SECONDS,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(234_000_000)
            .with_joined_community_fund_timestamp_seconds(Some(1))
            .with_maturity_e8s_equivalent(450_988_012)
            .with_neuron_type(Some(NeuronType::Ect as i32))
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                3,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: ONE_YEAR_SECONDS * 4,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(568_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                4,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: ONE_YEAR_SECONDS * 4,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(1_123_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                5,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: ONE_YEAR_SECONDS * 8,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(6_087_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                6,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: 5,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(0)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                7,
                DissolveStateAndAge::NotDissolving {
                    dissolve_delay_seconds: 5,
                    aging_since_timestamp_seconds: now,
                },
            )
            .with_cached_neuron_stake_e8s(100)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                8,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + ONE_YEAR_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(234_000_000)
            .with_staked_maturity_e8s_equivalent(100_000_000)
            .with_neuron_type(Some(NeuronType::Seed as i32))
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                9,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + ONE_YEAR_SECONDS * 3,
                },
            )
            .with_cached_neuron_stake_e8s(568_000_000)
            .with_staked_maturity_e8s_equivalent(100_000_000)
            .with_neuron_type(Some(NeuronType::Ect as i32))
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                10,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + ONE_YEAR_SECONDS * 5,
                },
            )
            .with_cached_neuron_stake_e8s(1_123_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                11,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + ONE_YEAR_SECONDS * 5,
                },
            )
            .with_cached_neuron_stake_e8s(6_087_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                12,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + ONE_YEAR_SECONDS * 7,
                },
            )
            .with_cached_neuron_stake_e8s(18_000_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                13,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - ONE_YEAR_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(4_450_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                14,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - ONE_YEAR_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(1_220_000_000)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                15,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: 1,
                },
            )
            .with_cached_neuron_stake_e8s(100_000_000)
            .build(),
        )
        .unwrap();
    // This neuron is inactive - not founded and dissolved "long ago".
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                16,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - ONE_YEAR_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(0)
            .build(),
        )
        .unwrap();
    // This neuron is spawning.
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                17,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now + 1,
                },
            )
            .with_cached_neuron_stake_e8s(100_000_000)
            .with_spawn_at_timestamp_seconds(now + 1)
            .build(),
        )
        .unwrap();

    let metrics = neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now);

    let expected_metrics = NeuronMetrics {
        dissolving_neurons_count: 5,
        dissolving_neurons_e8s_buckets: hashmap! {
            2 => 234000000.0,
            6 => 568000000.0,
            10 => 7210000000.0,
            14 => 18000000000.0
        },
        dissolving_neurons_count_buckets: hashmap! { 2 => 1, 6 => 1, 10 => 2, 14 => 1 },
        not_dissolving_neurons_count: 7,
        not_dissolving_neurons_e8s_buckets: hashmap! {
            0 => 100000100.0,
            2 => 234000000.0,
            8 => 1691000000.0,
            16 => 6087000000.0,
        },
        not_dissolving_neurons_count_buckets: hashmap! {0 => 3, 2 => 1, 8 => 2, 16 => 1},
        dissolved_neurons_count: 4,
        dissolved_neurons_e8s: 5770000000,
        garbage_collectable_neurons_count: 1,
        neurons_with_invalid_stake_count: 1,
        total_staked_e8s: 39_994_000_100,
        neurons_with_less_than_6_months_dissolve_delay_count: 8,
        neurons_with_less_than_6_months_dissolve_delay_e8s: 5970000100,
        community_fund_total_staked_e8s: 234_000_000,
        community_fund_total_maturity_e8s_equivalent: 450_988_012,
        neurons_fund_total_active_neurons: 1,
        total_locked_e8s: 34_224_000_100,
        total_maturity_e8s_equivalent: 450_988_012,
        total_staked_maturity_e8s_equivalent: 200_000_000_u64,
        dissolving_neurons_staked_maturity_e8s_equivalent_buckets: hashmap! {
            2 => 100000000.0,
            6 => 100000000.0,
            10 => 0.0,
            14 => 0.0,
        },
        dissolving_neurons_staked_maturity_e8s_equivalent_sum: 200_000_000_u64,
        not_dissolving_neurons_staked_maturity_e8s_equivalent_buckets: hashmap! {
            0 => 0.0,
            2 => 0.0,
            8 => 0.0,
            16 => 0.0,
        },
        not_dissolving_neurons_staked_maturity_e8s_equivalent_sum: 0_u64,
        seed_neuron_count: 2_u64,
        ect_neuron_count: 2_u64,
        total_staked_e8s_seed: 334000000,
        total_staked_e8s_ect: 802000000,
        total_staked_maturity_e8s_equivalent_seed: 100_000_000_u64,
        total_staked_maturity_e8s_equivalent_ect: 100_000_000_u64,
        dissolving_neurons_e8s_buckets_seed: hashmap! { 2 => 234000000.0 },
        dissolving_neurons_e8s_buckets_ect: hashmap! { 6 => 568000000.0 },
        not_dissolving_neurons_e8s_buckets_seed: hashmap! { 0 => 100000000.0 },
        not_dissolving_neurons_e8s_buckets_ect: hashmap! { 2 => 234000000.0 },
        spawning_neurons_count: 1,
        // Some garbage values, because this test was written before this feature.
        non_self_authenticating_controller_neuron_subset_metrics: NeuronSubsetMetrics::default(),
        public_neuron_subset_metrics: NeuronSubsetMetrics::default(),
        declining_voting_power_neuron_subset_metrics: NeuronSubsetMetrics::default(),
        fully_lost_voting_power_neuron_subset_metrics: NeuronSubsetMetrics::default(),
    };
    assert_eq!(
        NeuronMetrics {
            // Some garbage values, because this test was written before this feature.
            non_self_authenticating_controller_neuron_subset_metrics: NeuronSubsetMetrics::default(
            ),
            public_neuron_subset_metrics: NeuronSubsetMetrics::default(),
            declining_voting_power_neuron_subset_metrics: NeuronSubsetMetrics::default(),
            fully_lost_voting_power_neuron_subset_metrics: NeuronSubsetMetrics::default(),

            ..metrics
        },
        expected_metrics,
    );
}

#[test]
fn test_compute_metrics_inactive_neuron_in_heap() {
    // Step 1: prepare 3 neurons with different dissolved time: 1 day ago, 13 days ago, and 30
    // days ago.
    let mut neuron_store = NeuronStore::new(BTreeMap::new());
    let now = neuron_store.now();

    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                1,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - ONE_DAY_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(0)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                2,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - 13 * ONE_DAY_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(0)
            .build(),
        )
        .unwrap();
    neuron_store
        .add_neuron(
            NeuronBuilder::new_for_test(
                3,
                DissolveStateAndAge::DissolvingOrDissolved {
                    when_dissolved_timestamp_seconds: now - 30 * ONE_DAY_SECONDS,
                },
            )
            .with_cached_neuron_stake_e8s(0)
            .build(),
        )
        .unwrap();

    // Step 2: verify that 1 neuron (3) are inactive.
    let actual_metrics =
        neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now);
    assert_eq!(actual_metrics.garbage_collectable_neurons_count, 1);

    // Step 3: 2 days pass, and now neuron (2) is dissolved 15 days ago, and becomes inactive.
    let now = now + 2 * ONE_DAY_SECONDS;
    let actual_metrics =
        neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now);
    assert_eq!(actual_metrics.garbage_collectable_neurons_count, 2);
}

/// Tests rollups related to neurons with non-self-authenticating controller (basically,
/// canister-controlled neurons).
///
/// In this test, the NeuronStore has 4 neurons. The principal differences among them are as
/// follows:
///
///     1. Whether contoller is "normal" (self-authenticating), "weird" (non-self
///        authenticating; in practice, this means its a canister), or the Genesis Token
///        Canister (GTC).
///         a. Neurons 1 and 3 are weird. These (are supposed to) get counted.
///         b. Neuron 2 is normal. This is ignored.
///         c. Neuron 4 is controlled by the GTC.
///
///     2. They have different amounts of ICP-like resources (staked, staked maturity, and
///        maturity). The amounts are radically different to make it clearer what bug(s) might
///        exist if/when this test fails.
///
///     3. Voting power bonus factors (i.e. dissolve delay, and age).
#[test]
fn test_compute_neuron_metrics_non_self_authenticating() {
    // Step 1: Prepare the world.

    let now_seconds = 1718213756;

    // Step 1.1: Construct controllers (per this test's docstring).

    let controller_of_neuron_1 = PrincipalId::new_user_test_id(0x1337_CAFE);
    assert!(!controller_of_neuron_1.is_self_authenticating());

    let controller_of_neuron_2 =
        PrincipalId::from_str("ubktz-haghv-fqsdh-23fhi-3urex-bykoz-pvpfd-5rs6w-qpo3t-nf2dv-oae")
            .unwrap();
    assert!(controller_of_neuron_2.is_self_authenticating());

    let controller_of_neuron_3 = PrincipalId::from_str(
        // This is the NNS root canister's principal (canister) ID.
        "r7inp-6aaaa-aaaaa-aaabq-cai",
    )
    .unwrap();
    assert!(!controller_of_neuron_3.is_self_authenticating());

    // Step 1.2: Construct neurons (as described in the docstring).

    let neuron_1 = NeuronBuilder::new(
        NeuronId { id: 1 },
        Subaccount::try_from([1_u8; 32].as_ref()).unwrap(),
        controller_of_neuron_1,
        // Total voting power bonus: 2x * 1.125x = 2.25x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 8 * ONE_YEAR_SECONDS, // 100% (equivlanetly, 2x) dissolve delay bonus
            aging_since_timestamp_seconds: now_seconds - 2 * ONE_YEAR_SECONDS, // 12.5% (equivalently 1.125x) age bonus
        },
        now_seconds,
    )
    .with_cached_neuron_stake_e8s(100)
    .with_staked_maturity_e8s_equivalent(101)
    .with_maturity_e8s_equivalent(110)
    .build();

    let neuron_2 = NeuronBuilder::new(
        NeuronId { id: 2 },
        Subaccount::try_from([2_u8; 32].as_ref()).unwrap(),
        controller_of_neuron_2,
        // Total voting power bonus: 1.75x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 6 * ONE_YEAR_SECONDS, // 75% dissolve delay bonus.
            aging_since_timestamp_seconds: now_seconds,   // no age bonus.
        },
        now_seconds,
    )
    .with_cached_neuron_stake_e8s(200_000)
    .with_staked_maturity_e8s_equivalent(202_000)
    .with_maturity_e8s_equivalent(220_000)
    .build();

    let neuron_3 = NeuronBuilder::new(
        NeuronId { id: 3 },
        Subaccount::try_from([3_u8; 32].as_ref()).unwrap(),
        controller_of_neuron_3,
        // Total voting power bonus: 1.5x * 1.25x = 1.875x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 4 * ONE_YEAR_SECONDS, // 50% (equivalently, 1.5x) dissolve delay bonus
            aging_since_timestamp_seconds: now_seconds - 4 * ONE_YEAR_SECONDS, // 25% (equivalently 1.25x) age bonus
        },
        now_seconds,
    )
    .with_cached_neuron_stake_e8s(300_000_000)
    .with_staked_maturity_e8s_equivalent(303_000_000)
    .with_maturity_e8s_equivalent(330_000_000)
    .build();

    let neuron_4 = NeuronBuilder::new(
        NeuronId { id: 4 },
        Subaccount::try_from([4_u8; 32].as_ref()).unwrap(),
        PrincipalId::from(GENESIS_TOKEN_CANISTER_ID),
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 583 * ONE_DAY_SECONDS,
            aging_since_timestamp_seconds: now_seconds - 927 * ONE_DAY_SECONDS,
        },
        now_seconds - 335 * ONE_DAY_SECONDS,
    )
    .with_cached_neuron_stake_e8s(715_631_327)
    .with_staked_maturity_e8s_equivalent(281_771_001)
    .with_maturity_e8s_equivalent(988_862_650)
    .build();

    let voting_power_1 = neuron_1.potential_voting_power(now_seconds);
    let voting_power_3 = neuron_3.potential_voting_power(now_seconds);
    assert_eq!(voting_power_1, (2.250 * (100.0 + 101.0)) as u64);
    assert_eq!(
        voting_power_3,
        (1.875 * (300.0 + 303.0) * 1_000_000.0) as u64
    );

    // Step 1.3: Assemble neurons into collection.

    let neuron_store = NeuronStore::new(btreemap! {
        1 => neuron_1,
        2 => neuron_2,
        3 => neuron_3,
        4 => neuron_4,
    });

    // Step 2: Call code under test.

    let NeuronMetrics {
        non_self_authenticating_controller_neuron_subset_metrics,
        ..
    } = neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now_seconds);

    // Step 3: Inspect results.
    assert_eq!(
        non_self_authenticating_controller_neuron_subset_metrics,
        NeuronSubsetMetrics {
            count: 2,

            total_staked_e8s: 300_000_100,
            total_staked_maturity_e8s_equivalent: 303_000_101,
            total_maturity_e8s_equivalent: 330_000_110,

            // Voting power.
            total_voting_power: voting_power_1 + voting_power_3,
            total_deciding_voting_power: voting_power_1 + voting_power_3,
            total_potential_voting_power: voting_power_1 + voting_power_3,

            // Broken out by dissolve delay (rounded down to the nearest multiple of 6
            // months).

            // Analogous to the vanilla count field.
            count_buckets: hashmap! {
                8  => 1, // 1 neuron with 4 year dissolve delay.
                16 => 1, // 1 neuron with 8 year dissolve delay.
            },

            // ICP-like resources.
            staked_e8s_buckets: hashmap! {
                8  => 300_000_000,
                16 => 100,
            },
            staked_maturity_e8s_equivalent_buckets: hashmap! {
                8  => 303_000_000,
                16 => 101,
            },
            maturity_e8s_equivalent_buckets: hashmap! {
                8  => 330_000_000,
                16 => 110,
            },

            // Analogous to total_voting_power.
            voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
            deciding_voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
            potential_voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
        },
    );
}

/// Tests rollups related to public neurons.
///
/// There are three neurons in this test. Two of them are public, and one is
/// private. Of course, the private one does not contribute to the totals.
///
/// (If you are familiar with the cast in the previous test, the neurons here
/// should look pretty familiar.)
#[test]
fn test_compute_neuron_metrics_public_neurons() {
    // Step 1: Prepare the world.

    let now_seconds = 1718213756;

    // Step 1.1: Construct neurons (as described in the docstring).

    let neuron_1 = NeuronBuilder::new(
        NeuronId { id: 1 },
        Subaccount::try_from([1_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(1),
        // Total voting power bonus: 2x * 1.125x = 2.25x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 8 * ONE_YEAR_SECONDS, // 100% (equivlanetly, 2x) dissolve delay bonus
            aging_since_timestamp_seconds: now_seconds - 2 * ONE_YEAR_SECONDS, // 12.5% (equivalently 1.125x) age bonus
        },
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(100)
    .with_staked_maturity_e8s_equivalent(101)
    .with_maturity_e8s_equivalent(110)
    .with_visibility(Visibility::Public)
    .with_voting_power_refreshed_timestamp_seconds(now_seconds)
    .build();

    let neuron_2 = NeuronBuilder::new(
        NeuronId { id: 2 },
        Subaccount::try_from([2_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(2),
        // Total voting power bonus: 1.75x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 6 * ONE_YEAR_SECONDS, // 75% dissolve delay bonus.
            aging_since_timestamp_seconds: now_seconds,   // no age bonus.
        },
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(200_000)
    .with_staked_maturity_e8s_equivalent(202_000)
    .with_maturity_e8s_equivalent(220_000)
    .with_voting_power_refreshed_timestamp_seconds(now_seconds)
    .build();

    let neuron_3 = NeuronBuilder::new(
        NeuronId { id: 3 },
        Subaccount::try_from([3_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(3),
        // Total voting power bonus: 1.5x * 1.25x = 1.875x
        DissolveStateAndAge::NotDissolving {
            dissolve_delay_seconds: 4 * ONE_YEAR_SECONDS, // 50% (equivalently, 1.5x) dissolve delay bonus
            aging_since_timestamp_seconds: now_seconds - 4 * ONE_YEAR_SECONDS, // 25% (equivalently 1.25x) age bonus
        },
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(300_000_000)
    .with_staked_maturity_e8s_equivalent(303_000_000)
    .with_maturity_e8s_equivalent(330_000_000)
    // (Nominally) the neuron should be treated as public.
    .with_known_neuron_data(Some(KnownNeuronData {
        name: "Daniel Wong".to_string(),
        description: Some("Best engineer of all time. Of all time.".to_string()),
    }))
    .with_voting_power_refreshed_timestamp_seconds(now_seconds)
    .build();

    let voting_power_1 = neuron_1.potential_voting_power(now_seconds);
    let voting_power_3 = neuron_3.potential_voting_power(now_seconds);
    assert_eq!(voting_power_1, (2.250 * (100.0 + 101.0)) as u64);
    assert_eq!(
        voting_power_3,
        (1.875 * (300.0 + 303.0) * 1_000_000.0) as u64
    );

    // Step 1.2: Assemble neurons into collection.

    let neuron_store = NeuronStore::new(btreemap! {
        1 => neuron_1,
        2 => neuron_2,
        3 => neuron_3,
    });

    neuron_store
        .with_neuron(&NeuronId { id: 3 }, |neuron| {
            assert_eq!(neuron.visibility(), Visibility::Public, "{:#?}", neuron,);
        })
        .unwrap(); // Explode if neuron is not found.

    // Step 2: Call code under test.

    let NeuronMetrics {
        public_neuron_subset_metrics,
        ..
    } = neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now_seconds);

    // Step 3: Inspect results.
    assert_eq!(
        public_neuron_subset_metrics,
        NeuronSubsetMetrics {
            count: 2,

            total_staked_e8s: 300_000_100,
            total_staked_maturity_e8s_equivalent: 303_000_101,
            total_maturity_e8s_equivalent: 330_000_110,

            // Voting power.
            total_voting_power: voting_power_1 + voting_power_3,
            total_deciding_voting_power: voting_power_1 + voting_power_3,
            total_potential_voting_power: voting_power_1 + voting_power_3,

            // Broken out by dissolve delay (rounded down to the nearest multiple of 6
            // months).

            // Analogous to the vanilla count field.
            count_buckets: hashmap! {
                8  => 1, // 1 neuron with 4 year dissolve delay.
                16 => 1, // 1 neuron with 8 year dissolve delay.
            },

            // ICP-like resources.
            staked_e8s_buckets: hashmap! {
                8  => 300_000_000,
                16 => 100,
            },
            staked_maturity_e8s_equivalent_buckets: hashmap! {
                8  => 303_000_000,
                16 => 101,
            },
            maturity_e8s_equivalent_buckets: hashmap! {
                8  => 330_000_000,
                16 => 110,
            },

            // Analogous to total_voting_power.
            voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
            deciding_voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
            potential_voting_power_buckets: hashmap! {
                8  => voting_power_3,
                16 => voting_power_1,
            },
        },
    );
}

/// Tests rollups related to periodic refresh of neurons.
///
/// There are three neurons in this test:
///
///     1. Refreshed recently.
///
///     2. Refreshed 6.5 months ago. Thus, its deciding voting power is half of
///        its potential voting power.
///
///     3. Refreshed 8 months ago. Thus, deciding voting power is 0.
#[test]
fn test_compute_neuron_metrics_stale_and_expired_voting_power_neurons() {
    // Step 1: Prepare the world.

    let now_seconds = 1718213756;

    // Step 1.1: Construct neurons (as described in the docstring).

    // Total voting power bonus: 2x * 1.125x = 2.25x
    let dissolve_state_and_age = DissolveStateAndAge::NotDissolving {
        dissolve_delay_seconds: 8 * ONE_YEAR_SECONDS, // 100% (equivlanetly, 2x) dissolve delay bonus
        aging_since_timestamp_seconds: now_seconds - 2 * ONE_YEAR_SECONDS, // 12.5% (equivalently 1.125x) age bonus
    };
    let total_bonus_multiplier = 2.25;

    let fresh_neuron = NeuronBuilder::new(
        NeuronId { id: 1 },
        Subaccount::try_from([1_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(1),
        dissolve_state_and_age,
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(100)
    .with_staked_maturity_e8s_equivalent(101)
    .with_maturity_e8s_equivalent(110)
    .with_visibility(Visibility::Public)
    .with_voting_power_refreshed_timestamp_seconds(now_seconds)
    .build();

    let stale_neuron = NeuronBuilder::new(
        NeuronId { id: 2 },
        Subaccount::try_from([2_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(2),
        dissolve_state_and_age,
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(200_000)
    .with_staked_maturity_e8s_equivalent(202_000)
    .with_maturity_e8s_equivalent(220_000)
    .with_voting_power_refreshed_timestamp_seconds(
        now_seconds - 6 * ONE_MONTH_SECONDS - ONE_MONTH_SECONDS / 2,
    )
    .build();

    let expired_neuron = NeuronBuilder::new(
        NeuronId { id: 3 },
        Subaccount::try_from([3_u8; 32].as_ref()).unwrap(),
        PrincipalId::new_user_test_id(3),
        dissolve_state_and_age,
        now_seconds - 10 * ONE_YEAR_SECONDS,
    )
    .with_cached_neuron_stake_e8s(300_000_000)
    .with_staked_maturity_e8s_equivalent(303_000_000)
    .with_maturity_e8s_equivalent(330_000_000)
    .with_voting_power_refreshed_timestamp_seconds(now_seconds - 8 * ONE_MONTH_SECONDS)
    .build();

    let fresh_potential_voting_power = fresh_neuron.potential_voting_power(now_seconds);
    let stale_potential_voting_power = stale_neuron.potential_voting_power(now_seconds);
    let expired_potential_voting_power = expired_neuron.potential_voting_power(now_seconds);
    assert_eq!(
        fresh_potential_voting_power,
        (total_bonus_multiplier * (100.0 + 101.0)) as u64
    );
    assert_eq!(
        stale_potential_voting_power,
        (total_bonus_multiplier * (200.0e3 + 202.0e3)) as u64
    );
    assert_eq!(
        expired_potential_voting_power,
        (total_bonus_multiplier * (300.0 + 303.0) * 1e6) as u64
    );

    // Step 1.2: Assemble neurons into collection.

    let neuron_store = NeuronStore::new(btreemap! {
        fresh_neuron.id().id => fresh_neuron,
        stale_neuron.id().id => stale_neuron,
        expired_neuron.id().id => expired_neuron,
    });

    // Step 2: Call code under test.

    let NeuronMetrics {
        declining_voting_power_neuron_subset_metrics,
        fully_lost_voting_power_neuron_subset_metrics,
        ..
    } = neuron_store.compute_neuron_metrics(E8, &VotingPowerEconomics::DEFAULT, now_seconds);

    // Step 3: Inspect results.

    assert_eq!(
        declining_voting_power_neuron_subset_metrics,
        NeuronSubsetMetrics {
            count: 1,

            // Here, we are seeing a pretty good indicator that stale neuron
            // detection is working, because there isn't a plausible alternative
            // explanation for how these values can be achieved from the numbers
            // that we fed into the stats compiler. Ofc, other fields also
            // indicate that stale neuron detection works.
            total_staked_e8s: 200_000,
            total_staked_maturity_e8s_equivalent: 202_000,
            total_maturity_e8s_equivalent: 220_000,

            // Voting power. Here, we see "good" evidence that the "right"
            // voting power (deciding vs. voting) is used to populate these
            // fields.
            total_voting_power: stale_potential_voting_power,
            total_deciding_voting_power: stale_potential_voting_power / 2,
            total_potential_voting_power: stale_potential_voting_power,

            // Broken out by dissolve delay (rounded down to the nearest multiple of 6
            // months).

            // Analogous to the vanilla count field.
            count_buckets: hashmap! {
                16 => 1, // 1 neuron with 4 year dissolve delay.
            },

            // ICP-like resources.
            staked_e8s_buckets: hashmap! {
                16 => 200_000,
            },
            staked_maturity_e8s_equivalent_buckets: hashmap! {
                16 => 202_000,
            },
            maturity_e8s_equivalent_buckets: hashmap! {
                16 => 220_000,
            },

            // Analogous to total_voting_power.
            voting_power_buckets: hashmap! {
                16 => stale_potential_voting_power,
            },
            // Ditto earlier comments about "right" voting power.
            deciding_voting_power_buckets: hashmap! {
                16 => stale_potential_voting_power / 2,
            },
            potential_voting_power_buckets: hashmap! {
                16 => stale_potential_voting_power,
            },
        },
    );

    assert_eq!(
        fully_lost_voting_power_neuron_subset_metrics,
        NeuronSubsetMetrics {
            count: 1,

            // Similar to the previous assert, this indicates that expired
            // neuron detection works.
            total_staked_e8s: 300_000_000,
            total_staked_maturity_e8s_equivalent: 303_000_000,
            total_maturity_e8s_equivalent: 330_000_000,

            // Voting power.
            total_voting_power: expired_potential_voting_power,
            // Similar to the previous assert, this indicates that the "right"
            // (deciding vs. potential) voting power is being used.
            total_deciding_voting_power: 0,
            total_potential_voting_power: expired_potential_voting_power,

            // Broken out by dissolve delay (rounded down to the nearest multiple of 6
            // months).

            // Analogous to the vanilla count field.
            count_buckets: hashmap! {
                16 => 1, // 1 neuron with 4 year dissolve delay.
            },

            // ICP-like resources.
            staked_e8s_buckets: hashmap! {
                16 => 300_000_000,
            },
            staked_maturity_e8s_equivalent_buckets: hashmap! {
                16 => 303_000_000,
            },
            maturity_e8s_equivalent_buckets: hashmap! {
                16 => 330_000_000,
            },

            // Analogous to total_voting_power.
            voting_power_buckets: hashmap! {
                16 => expired_potential_voting_power,
            },
            // Ditto earlier comment about "right" voting power.
            deciding_voting_power_buckets: hashmap! {
                16 => 0,
            },
            potential_voting_power_buckets: hashmap! {
                16 => expired_potential_voting_power,
            },
        },
    );
}
