
//! Autogenerated weights for `pallet_bridge`
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2022-10-20, STEPS: `50`, REPEAT: 20, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! HOSTNAME: `ip-172-88-3-164`, CPU: `Intel(R) Xeon(R) Platinum 8124M CPU @ 3.00GHz`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("parallel-dev"), DB CACHE: 1024

// Executed Command:
// ./target/release/parallel
// benchmark
// pallet
// --chain=parallel-dev
// --execution=wasm
// --wasm-execution=compiled
// --pallet=pallet_bridge
// --extrinsic=*
// --steps=50
// --repeat=20
// --output=./runtime/parallel/src/weights/pallet_bridge.rs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

/// Weight functions for `pallet_bridge`.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> pallet_bridge::WeightInfo for WeightInfo<T> {
	// Storage: Bridge ChainNonces (r:1 w:1)
	// Storage: Bridge BridgeRegistry (r:0 w:1)
	fn register_chain() -> Weight {
		(32_915_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(1 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: Bridge ChainNonces (r:1 w:1)
	// Storage: Bridge BridgeRegistry (r:0 w:1)
	fn unregister_chain() -> Weight {
		(32_213_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(1 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: Bridge BridgeTokens (r:1 w:1)
	// Storage: Bridge AssetIds (r:1 w:1)
	fn register_bridge_token() -> Weight {
		(36_661_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: Bridge AssetIds (r:1 w:1)
	// Storage: Bridge BridgeTokens (r:0 w:1)
	fn unregister_bridge_token() -> Weight {
		(34_383_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(1 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	fn set_bridge_token_fee() -> Weight {
		(39_103_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	fn set_bridge_token_status() -> Weight {
		(38_088_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	fn set_bridge_token_cap() -> Weight {
		(38_428_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	fn clean_cap_accumulated_value() -> Weight {
		(38_176_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: Bridge ChainNonces (r:1 w:1)
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	// Storage: System Account (r:1 w:1)
	fn teleport() -> Weight {
		(100_605_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(4 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	// Storage: Bridge ChainNonces (r:1 w:0)
	// Storage: Bridge BridgeRegistry (r:1 w:1)
	// Storage: Bridge AssetIds (r:1 w:0)
	// Storage: Bridge BridgeTokens (r:1 w:1)
	// Storage: Bridge ProposalVotes (r:1 w:1)
	// Storage: Bridge VoteThreshold (r:1 w:0)
	// Storage: BridgeMembership Members (r:1 w:0)
	// Storage: System Account (r:2 w:1)
	// Storage: Assets Metadata (r:1 w:0)
	fn materialize() -> Weight {
		(161_366_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(10 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
}
