// This file is part of Parallel Finance.

// Copyright (C) 2022 Parallel Finance Developer.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for pallet_asset_registry
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2022-05-30, STEPS: `50`, REPEAT: 20, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("kerria-dev"), DB CACHE: 1024

// Executed Command:
// ./target/release/parallel
// benchmark
// pallet
// --chain=kerria-dev
// --execution=wasm
// --wasm-execution=compiled
// --pallet=pallet-asset-registry
// --extrinsic=*
// --steps=50
// --repeat=20
// --heap-pages=4096
// --template=./.maintain/frame-weight-template.hbs
// --output=./pallets/asset-registry/src/weights.rs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_asset_registry.
pub trait WeightInfo {
	fn register_asset() -> Weight;
	fn update_asset_units_per_second() -> Weight;
	fn update_asset_type() -> Weight;
	fn remove_fee_payment_asset() -> Weight;
	fn deregister_asset() -> Weight;
}

/// Weights for pallet_asset_registry using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeId (r:0 w:1)
	fn register_asset() -> Weight {
		(36_449_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(1 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry AssetTypeId (r:1 w:0)
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	fn update_asset_units_per_second() -> Weight {
		(45_578_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:1 w:2)
	// Storage: AssetRegistry AssetTypeId (r:0 w:2)
	fn update_asset_type() -> Weight {
		(61_904_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(3 as Weight))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	fn remove_fee_payment_asset() -> Weight {
		(38_682_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(1 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	// Storage: AssetRegistry AssetTypeId (r:0 w:1)
	fn deregister_asset() -> Weight {
		(48_719_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(2 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeId (r:0 w:1)
	fn register_asset() -> Weight {
		(36_449_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(1 as Weight))
			.saturating_add(RocksDbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry AssetTypeId (r:1 w:0)
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	fn update_asset_units_per_second() -> Weight {
		(45_578_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(2 as Weight))
			.saturating_add(RocksDbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:1 w:2)
	// Storage: AssetRegistry AssetTypeId (r:0 w:2)
	fn update_asset_type() -> Weight {
		(61_904_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(3 as Weight))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	fn remove_fee_payment_asset() -> Weight {
		(38_682_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(1 as Weight))
			.saturating_add(RocksDbWeight::get().writes(2 as Weight))
	}
	// Storage: AssetRegistry SupportedFeePaymentAssets (r:1 w:1)
	// Storage: AssetRegistry AssetIdType (r:1 w:1)
	// Storage: AssetRegistry AssetTypeUnitsPerSecond (r:0 w:1)
	// Storage: AssetRegistry AssetTypeId (r:0 w:1)
	fn deregister_asset() -> Weight {
		(48_719_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(2 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
}
