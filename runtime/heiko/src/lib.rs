// Copyright 2021 Parallel Finance Developer.
// This file is part of Parallel Finance.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    construct_runtime,
    dispatch::{DispatchResult, Weight},
    log, match_types, parameter_types,
    traits::{
        fungibles::{InspectMetadata, Mutate},
        tokens::BalanceConversion,
        ChangeMembers, ConstU32, Contains, EitherOfDiverse, EqualPrivilegeOnly, Everything,
        InstanceFilter, Nothing,
    },
    weights::{
        constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_PER_SECOND},
        ConstantMultiplier, DispatchClass,
    },
    PalletId, WeakBoundedVec,
};
use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureRoot,
};
use orml_traits::{
    location::AbsoluteReserveProvider, parameter_type_with_key, DataFeeder, DataProvider,
    DataProviderExtended,
};
use orml_xcm_support::{IsNativeConcrete, MultiNativeAsset};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::SlowAdjustingFeeUpdate;
use scale_info::TypeInfo;
use sp_api::impl_runtime_apis;
use sp_core::OpaqueMetadata;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
    create_runtime_str, generic, impl_opaque_keys,
    traits::{
        self, AccountIdConversion, AccountIdLookup, BlakeTwo256, Block as BlockT,
        BlockNumberProvider, Convert, Zero,
    },
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, DispatchError, FixedPointNumber, KeyTypeId, Perbill, Permill,
    RuntimeDebug, SaturatedConversion,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;
use xcm::latest::prelude::*;
use xcm_builder::{
    AccountId32Aliases, AllowKnownQueryResponses, AllowSubscriptionsFrom,
    AllowTopLevelPaidExecutionFrom, ConvertedConcreteAssetId, EnsureXcmOrigin, FixedWeightBounds,
    FungiblesAdapter, LocationInverter, ParentAsSuperuser, ParentIsPreset, RelayChainAsNative,
    SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
    SignedToAccountId32, SovereignSignedViaLocation, TakeRevenue, TakeWeightCredit,
};
use xcm_executor::{traits::JustTry, Config, XcmExecutor};

// A few exports that help ease life for downstream crates.
// re-exports
mod weights;

pub mod constants;

use constants::{currency, fee, time};
use currency::*;
use fee::*;
use time::*;

pub use pallet_amm;
pub use pallet_asset_registry;
pub use pallet_bridge;
pub use pallet_crowdloans;
pub use pallet_farming;
pub use pallet_liquid_staking;
pub use pallet_loans;
pub use pallet_prices;
pub use pallet_router;
pub use pallet_streaming;

use pallet_traits::{
    xcm::{
        AccountIdToMultiLocation, AsAssetType, AssetType, CurrencyIdConvert, FirstAssetTrader,
        MultiCurrencyAdapter,
    },
    DecimalProvider, EmergencyCallFilter, ValidationDataProvider,
};
use primitives::{
    network::HEIKO_PREFIX,
    paras,
    tokens::{EUSDC, EUSDT, HKO, KSM, SKSM},
    AccountId, AuraId, Balance, BlockNumber, ChainId, CurrencyId, DataProviderId, EraIndex, Hash,
    Index, Liquidity, Moment, PersistedValidationData, Price, Rate, Ratio, Shortfall, Signature,
    KSM_U,
};

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

    use super::*;

    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;

    pub type SessionHandlers = ();

    impl_opaque_keys! {
        pub struct SessionKeys {
            pub aura: Aura,
        }
    }
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("heiko"),
    impl_name: create_runtime_str!("heiko"),
    authoring_version: 1,
    spec_version: 194,
    impl_version: 33,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 17,
    state_version: 0,
};

// 1 in 4 blocks (on average, not counting collisions) will be primary babe blocks.
pub const PRIMARY_PROBABILITY: (u64, u64) = (1, 4);

pub const NATIVE_ASSET_ID: u32 = HKO;

#[derive(codec::Encode, codec::Decode)]
pub enum XCMPMessage<XAccountId, XBalance> {
    /// Transfer tokens to the given account from the Parachain account.
    TransferToken(XAccountId, XBalance),
}

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

/// We assume that ~10% of the block weight is consumed by `on_initialize` handlers.
/// This is used to limit the maximal weight of a single extrinsic.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);
/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used
/// by  Operational  extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
/// We allow for 500 ms of compute with parachain block.
const MAXIMUM_BLOCK_WEIGHT: Weight = WEIGHT_PER_SECOND / 2;

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const Version: RuntimeVersion = VERSION;
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved = Some(
                MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
            );
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic();
    pub const SS58Prefix: u8 = HEIKO_PREFIX;
}

pub struct WhiteListFilter;
impl Contains<Call> for WhiteListFilter {
    fn contains(call: &Call) -> bool {
        matches!(
            call,
            // System, Currencies
            Call::System(_) |
            Call::Timestamp(_) |
            Call::Assets(pallet_assets::Call::force_create { .. }) |
            Call::Assets(pallet_assets::Call::force_set_metadata { .. }) |
            Call::Assets(pallet_assets::Call::force_asset_status { .. }) |
            // Governance
            // Call::Sudo(_) |
            Call::Democracy(_) |
            Call::GeneralCouncil(_) |
            Call::TechnicalCommittee(_) |
            Call::Treasury(_) |
            Call::Scheduler(_) |
            Call::Preimage(_) |
            // Parachain
            Call::ParachainSystem(_) |
            Call::XcmpQueue(_) |
            Call::DmpQueue(_) |
            Call::PolkadotXcm(pallet_xcm::Call::force_xcm_version { .. }) |
            Call::PolkadotXcm(pallet_xcm::Call::force_default_xcm_version { .. }) |
            Call::PolkadotXcm(pallet_xcm::Call::force_subscribe_version_notify { .. }) |
            Call::PolkadotXcm(pallet_xcm::Call::force_unsubscribe_version_notify { .. }) |
            Call::CumulusXcm(_) |
            // Consensus
            Call::Authorship(_) |
            Call::Session(_) |
            // Call::CollatorSelection(_) |
            // Utility
            Call::Utility(_) |
            Call::Multisig(_) |
            Call::Proxy(_) |
            Call::Identity(_) |
            Call::EmergencyShutdown(_) |
            Call::XcmHelper(_) |
            // Membership
            Call::OracleMembership(_) |
            Call::GeneralCouncilMembership(_) |
            Call::TechnicalCommitteeMembership(_) |
            Call::LiquidStakingAgentsMembership(_) |
            Call::CrowdloansAutomatorsMembership(_) |
            Call::BridgeMembership(_)
        )
    }
}

pub struct BaseCallFilter;
impl Contains<Call> for BaseCallFilter {
    fn contains(call: &Call) -> bool {
        (WhiteListFilter::contains(call)
            || matches!(
                call,
                // System, Currencies
                Call::Balances(_) |
                Call::Assets(pallet_assets::Call::mint { .. }) |
                Call::Assets(pallet_assets::Call::transfer { .. }) |
                Call::Assets(pallet_assets::Call::transfer_keep_alive { .. }) |
                Call::Assets(pallet_assets::Call::freeze { .. }) |
                Call::Assets(pallet_assets::Call::thaw { .. }) |
                Call::Assets(pallet_assets::Call::freeze_asset { .. }) |
                Call::Assets(pallet_assets::Call::thaw_asset { .. }) |
                Call::Assets(pallet_assets::Call::burn { .. }) |
                Call::Assets(pallet_assets::Call::destroy { .. }) |
                Call::CurrencyAdapter(_) |
                // 3rd Party
                Call::Vesting(_) |
                Call::Oracle(_) |
                Call::XTokens(_) |
                Call::OrmlXcm(_) |
                // Loans
                Call::Loans(_) |
                Call::Prices(_) |
                // AMM
                Call::AMM(_) |
                Call::AMMRoute(_) |
                // Crowdloans
                Call::Crowdloans(_) |
                // Liquid Staking
                Call::LiquidStaking(_) |
                // Bridge
                Call::Bridge(_) |
                // Farming
                Call::Farming(_) |
                // Streaming
                Call::Streaming(_) |
                // Asset Management
                Call::AssetRegistry(_)
            ))
            && EmergencyShutdown::contains(call)
    }
}

impl frame_system::Config for Runtime {
    /// The basic call filter to use in dispatchable.
    type BaseCallFilter = BaseCallFilter;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The aggregated dispatch type that is available for extrinsics.
    type Call = Call;
    /// The lookup mechanism to get account ID from whatever is passed in dispatchers.
    type Lookup = AccountIdLookup<AccountId, ()>;
    /// The index type for storing how many extrinsics an account has signed.
    type Index = Index;
    /// The index type for blocks.
    type BlockNumber = BlockNumber;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The hashing algorithm used.
    type Hashing = BlakeTwo256;
    /// The header type.
    type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// The ubiquitous event type.
    type Event = Event;
    /// The ubiquitous origin type.
    type Origin = Origin;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// Converts a module to the index of the module in `construct_runtime!`.
    ///
    /// This type is being generated by `construct_runtime!`.
    type PalletInfo = PalletInfo;
    /// What to do if a new account is created.
    type OnNewAccount = ();
    /// What to do if an account is fully reaped from the system.
    type OnKilledAccount = ();
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// Weight information for the extrinsics of this pallet.
    type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The set code logic.
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
    pub TreasuryAccount: AccountId = TreasuryPalletId::get().into_account_truncating();
}

impl orml_xcm::Config for Runtime {
    type Event = Event;
    type SovereignOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
}

parameter_types! {
    pub const LoansPalletId: PalletId = PalletId(*b"par/loan");
}

parameter_types! {
    pub SelfLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(ParachainInfo::parachain_id().into())));
    pub const BaseXcmWeight: Weight = 150_000_000;
    pub const MaxInstructions: u32 = 100;
    pub const MaxAssetsForTransfer: usize = 2;
}

// Min fee required when transferring asset back to reserve sibling chain
// which use another asset(e.g Relaychain's asset) as fee
parameter_type_with_key! {
    pub ParachainMinFee: |location: MultiLocation| -> Option<u128> {
        #[allow(clippy::match_ref_pats)] // false positive
        match (location.parents, location.first_interior()) {
            (1, Some(Parachain(paras::statemine::ID))) => Some(XcmHelper::get_xcm_weight_fee_to_sibling(location.clone()).fee),//default fee should be enough even if not configured
            _ => None,
        }
    };
}

impl orml_xtokens::Config for Runtime {
    type Event = Event;
    type Balance = Balance;
    type CurrencyId = CurrencyId;
    type CurrencyIdConvert = CurrencyIdConvert<AssetRegistry>;
    type AccountIdToMultiLocation = AccountIdToMultiLocation<AccountId>;
    type SelfLocation = SelfLocation;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type Weigher = FixedWeightBounds<BaseXcmWeight, Call, MaxInstructions>;
    type BaseXcmWeight = BaseXcmWeight;
    type LocationInverter = LocationInverter<Ancestry>;
    type MaxAssetsForTransfer = MaxAssetsForTransfer;
    type MinXcmFee = ParachainMinFee;
    type MultiLocationsFilter = Everything;
    type ReserveProvider = AbsoluteReserveProvider;
}

parameter_types! {
    pub const AssetDeposit: Balance = DOLLARS; // 1 UNIT deposit to create asset
    pub const ApprovalDeposit: Balance = EXISTENTIAL_DEPOSIT;
    pub const AssetsStringLimit: u32 = 50;
    pub const AssetAccountDeposit: Balance = deposit(1, 16);
    /// Key = 32 bytes, Value = 36 bytes (32+1+1+1+1)
    // https://github.com/paritytech/substrate/blob/069917b/frame/assets/src/lib.rs#L257L271
    pub const MetadataDepositBase: Balance = deposit(1, 68);
    pub const MetadataDepositPerByte: Balance = deposit(0, 1);
}

impl pallet_assets::Config for Runtime {
    type Event = Event;
    type Balance = Balance;
    type AssetId = CurrencyId;
    type Currency = Balances;
    type ForceOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type AssetDeposit = AssetDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type AssetAccountDeposit = AssetAccountDeposit;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type WeightInfo = weights::pallet_assets::WeightInfo<Runtime>;
    type Extra = ();
}

parameter_types! {
    pub const RewardAssetId: CurrencyId = HKO;
    pub const LiquidationFreeAssetId: CurrencyId = KSM;
}

impl pallet_loans::Config for Runtime {
    type Event = Event;
    type PalletId = LoansPalletId;
    type PriceFeeder = Prices;
    type ReserveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_loans::WeightInfo<Runtime>;
    type UnixTime = Timestamp;
    type Assets = CurrencyAdapter;
    type RewardAssetId = RewardAssetId;
    type LiquidationFreeAssetId = LiquidationFreeAssetId;
}

parameter_types! {
    pub const StakingPalletId: PalletId = PalletId(*b"par/lqsk");
    pub const EraLength: BlockNumber = 6 * 1 * 3600 / 6; // 6HOURS
    pub const MinStake: Balance = 100_000_000_000; // 0.1KSM
    pub const MinUnstake: Balance = 50_000_000_000; // 0.05sKSM
    pub const StakingCurrency: CurrencyId = KSM;
    pub const LiquidCurrency: CurrencyId = SKSM;
    pub const CollateralCurrency: CurrencyId = KSM_U;
    pub const XcmFees: Balance = 5_000_000_000; // 0.005KSM
    // delay 7 eras, we must be able to repay in less than 7 eras
    pub LoansInstantUnstakeFee: Rate = Rate::saturating_from_rational(1u32, 100u32); // (1.5 ** (3600 * 36 / 5256000) - 1) * 100% ~= 1.004%
    pub MatchingPoolFastUnstakeFee: Rate = Rate::saturating_from_rational(1u32, 100u32);
    pub const BondingDuration: EraIndex = 28; // 7Days
    pub const MinNominatorBond: Balance = 100_000_000_000; // 0.1KSM
    pub const NumSlashingSpans: u32 = 0;
    pub DerivativeIndexList: Vec<u16> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    pub const ElectionSolutionStoredOffset: BlockNumber = 3150;
}

impl pallet_liquid_staking::Config for Runtime {
    type Event = Event;
    type Origin = Origin;
    type Call = Call;
    type PalletId = StakingPalletId;
    type LoansPalletId = LoansPalletId;
    type WeightInfo = weights::pallet_liquid_staking::WeightInfo<Runtime>;
    type SelfParaId = ParachainInfo;
    type Assets = Assets;
    type RelayOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type DerivativeIndexList = DerivativeIndexList;
    type DistributionStrategy = pallet_liquid_staking::distribution::MaxMinDistribution;
    type XcmFees = XcmFees;
    type LoansInstantUnstakeFee = LoansInstantUnstakeFee;
    type MatchingPoolFastUnstakeFee = MatchingPoolFastUnstakeFee;
    type StakingCurrency = StakingCurrency;
    type LiquidCurrency = LiquidCurrency;
    type CollateralCurrency = CollateralCurrency;
    type EraLength = EraLength;
    type MinStake = MinStake;
    type MinUnstake = MinUnstake;
    type XCM = XcmHelper;
    type BondingDuration = BondingDuration;
    type MinNominatorBond = MinNominatorBond;
    type RelayChainValidationDataProvider = RelayChainValidationDataProvider<Runtime>;
    type Loans = Loans;
    type Members = LiquidStakingAgentsMembership;
    type NumSlashingSpans = NumSlashingSpans;
    type ElectionSolutionStoredOffset = ElectionSolutionStoredOffset;
    type ProtocolFeeReceiver = DefaultProtocolFeeReceiver;
}

parameter_types! {
    pub const LiquidStakingAgentsMembershipMaxMembers: u32 = 100;
}

type LiquidStakingAgentsMembershipInstance = pallet_membership::Instance5;
impl pallet_membership::Config<LiquidStakingAgentsMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = ();
    type MembershipChanged = ();
    type MaxMembers = LiquidStakingAgentsMembershipMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

parameter_types! {
    pub const CrowdloansAutomatorsMembershipMaxMembers: u32 = 100;
}

type CrowdloansAutomatorsMembershipInstance = pallet_membership::Instance7;
impl pallet_membership::Config<CrowdloansAutomatorsMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = ();
    type MembershipChanged = ();
    type MaxMembers = CrowdloansAutomatorsMembershipMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
    Call: From<LocalCall>,
{
    fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
        call: Call,
        public: <Signature as traits::Verify>::Signer,
        account: AccountId,
        index: Index,
    ) -> Option<(
        Call,
        <UncheckedExtrinsic as traits::Extrinsic>::SignaturePayload,
    )> {
        let period = BlockHashCount::get() as u64;
        let current_block = System::block_number()
            .saturated_into::<u64>()
            .saturating_sub(1);
        let tip = 0;
        let extra: SignedExtra = (
            frame_system::CheckNonZeroSender::<Runtime>::new(),
            frame_system::CheckSpecVersion::<Runtime>::new(),
            frame_system::CheckTxVersion::<Runtime>::new(),
            frame_system::CheckGenesis::<Runtime>::new(),
            frame_system::CheckEra::<Runtime>::from(generic::Era::mortal(period, current_block)),
            frame_system::CheckNonce::<Runtime>::from(index),
            frame_system::CheckWeight::<Runtime>::new(),
            pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
        );

        let raw_payload = SignedPayload::new(call, extra)
            .map_err(|e| {
                log::error!("SignedPayload error: {:?}", e);
            })
            .ok()?;
        let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
        let address = account;
        let (call, extra, _) = raw_payload.deconstruct();
        Some((
            call,
            (sp_runtime::MultiAddress::Id(address), signature, extra),
        ))
    }
}

impl frame_system::offchain::SigningTypes for Runtime {
    type Public = <Signature as traits::Verify>::Signer;
    type Signature = Signature;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
    Call: From<C>,
{
    type OverarchingCall = Call;
    type Extrinsic = UncheckedExtrinsic;
}

parameter_types! {
    pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = weights::pallet_timestamp::WeightInfo<Runtime>;
}

parameter_types! {
    pub const UncleGenerations: u32 = 0;
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type UncleGenerations = UncleGenerations;
    type FilterUncle = ();
    type EventHandler = (CollatorSelection,);
}

parameter_types! {
    pub const Period: u32 = 6 * HOURS;
    pub const Offset: u32 = 0;
}

impl pallet_session::Config for Runtime {
    type Event = Event;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    // we don't have stash and controller, thus we don't need the convert as well.
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = CollatorSelection;
    // Essentially just Aura, but lets be pedantic.
    type SessionHandler =
        <opaque::SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = opaque::SessionKeys;
    type WeightInfo = ();
}

parameter_types! {
    pub const PotId: PalletId = PalletId(*b"par/pstk");
    pub const MaxCandidates: u32 = 1000;
    pub const MinCandidates: u32 = 1;
    pub const MaxInvulnerables: u32 = 100;
}

impl pallet_collator_selection::Config for Runtime {
    type Event = Event;
    type Currency = Balances;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PotId = PotId;
    type MaxCandidates = MaxCandidates;
    type MinCandidates = MinCandidates;
    type MaxInvulnerables = MaxInvulnerables;
    // should be a multiple of session or things will get inconsistent
    type KickThreshold = Period;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ValidatorRegistration = Session;
    type WeightInfo = ();
}

parameter_types! {
    pub const MaxAuthorities: u32 = 100_000;
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = MaxAuthorities;
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

parameter_types! {
    pub const ExistentialDeposit: u128 = currency::EXISTENTIAL_DEPOSIT;
    pub const MaxLocks: u32 = 50;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = MaxLocks;
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type Event = Event;
    type DustRemoval = Treasury;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}

parameter_types! {
    pub const TransactionByteFee: Balance = 1 * MILLICENTS;
    pub const OperationalFeeMultiplier: u8 = 5;
}

impl pallet_transaction_payment::Config for Runtime {
    type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, Treasury>;
    type WeightToFee = WeightToFee;
    type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
    type OperationalFeeMultiplier = OperationalFeeMultiplier;
    type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
    type Event = Event;
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Encode,
    Decode,
    RuntimeDebug,
    MaxEncodedLen,
    TypeInfo,
)]
pub enum ProxyType {
    Any,
    Loans,
    Staking,
    Crowdloans,
    Farming,
    Streaming,
    Governance,
    AMM,
}
impl Default for ProxyType {
    fn default() -> Self {
        Self::Any
    }
}

impl InstanceFilter<Call> for ProxyType {
    fn filter(&self, c: &Call) -> bool {
        match self {
            ProxyType::Any => true,
            ProxyType::Loans => {
                matches!(
                    c,
                    Call::Loans(pallet_loans::Call::mint { .. })
                        | Call::Loans(pallet_loans::Call::redeem { .. })
                        | Call::Loans(pallet_loans::Call::redeem_all { .. })
                        | Call::Loans(pallet_loans::Call::borrow { .. })
                        | Call::Loans(pallet_loans::Call::repay_borrow { .. })
                        | Call::Loans(pallet_loans::Call::repay_borrow_all { .. })
                        | Call::Loans(pallet_loans::Call::collateral_asset { .. })
                        | Call::Loans(pallet_loans::Call::liquidate_borrow { .. })
                        | Call::Loans(pallet_loans::Call::add_reward { .. })
                        | Call::Loans(pallet_loans::Call::claim_reward { .. })
                        | Call::Loans(pallet_loans::Call::claim_reward_for_market { .. })
                )
            }
            ProxyType::Staking => {
                matches!(
                    c,
                    Call::LiquidStaking(pallet_liquid_staking::Call::stake { .. })
                        | Call::LiquidStaking(pallet_liquid_staking::Call::unstake { .. })
                        | Call::LiquidStaking(pallet_liquid_staking::Call::cancel_unstake { .. })
                )
            }
            ProxyType::Crowdloans => {
                matches!(
                    c,
                    Call::Crowdloans(pallet_crowdloans::Call::contribute { .. },)
                        | Call::Crowdloans(pallet_crowdloans::Call::claim { .. })
                        | Call::Crowdloans(pallet_crowdloans::Call::claim_for { .. })
                        | Call::Crowdloans(pallet_crowdloans::Call::withdraw { .. })
                        | Call::Crowdloans(pallet_crowdloans::Call::withdraw_for { .. })
                        | Call::Crowdloans(pallet_crowdloans::Call::redeem { .. })
                )
            }
            ProxyType::Farming => {
                matches!(
                    c,
                    Call::Farming(pallet_farming::Call::deposit { .. })
                        | Call::Farming(pallet_farming::Call::claim { .. })
                        | Call::Farming(pallet_farming::Call::withdraw { .. })
                        | Call::Farming(pallet_farming::Call::redeem { .. })
                )
            }
            ProxyType::Streaming => {
                matches!(
                    c,
                    Call::Streaming(pallet_streaming::Call::create { .. })
                        | Call::Streaming(pallet_streaming::Call::cancel { .. })
                        | Call::Streaming(pallet_streaming::Call::withdraw { .. })
                )
            }
            ProxyType::Governance => {
                matches!(
                    c,
                    Call::Democracy(..)
                        | Call::Preimage(..)
                        | Call::GeneralCouncil(..)
                        | Call::TechnicalCommittee(..)
                        | Call::Treasury(..)
                        | Call::Utility(..)
                )
            }
            ProxyType::AMM => {
                matches!(
                    c,
                    Call::AMM(pallet_amm::Call::add_liquidity { .. })
                        | Call::AMM(pallet_amm::Call::remove_liquidity { .. })
                        | Call::AMMRoute(pallet_router::Call::swap_tokens_for_exact_tokens { .. })
                        | Call::AMMRoute(pallet_router::Call::swap_exact_tokens_for_tokens { .. })
                )
            }
        }
    }
    fn is_superset(&self, o: &Self) -> bool {
        match (self, o) {
            (ProxyType::Any, _) => true,
            (_, ProxyType::Any) => false,
            _ => false,
        }
    }
}

parameter_types! {
    // One storage item; key size 32, value size 8; .
    pub const ProxyDepositBase: Balance = deposit(1, 40);
    // Additional storage item size of 33 bytes.
    pub const ProxyDepositFactor: Balance = deposit(0, 33);
    pub const MaxProxies: u16 = 32;
    // One storage item; key size 32, value size 16
    pub const AnnouncementDepositBase: Balance = deposit(1, 48);
    pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
    pub const MaxPending: u16 = 32;
}

impl pallet_proxy::Config for Runtime {
    type Event = Event;
    type Call = Call;
    type Currency = Balances;
    type ProxyType = ProxyType;
    type ProxyDepositBase = ProxyDepositBase;
    type ProxyDepositFactor = ProxyDepositFactor;
    type MaxProxies = MaxProxies;
    type WeightInfo = weights::pallet_proxy::WeightInfo<Runtime>;
    type MaxPending = MaxPending;
    type CallHasher = BlakeTwo256;
    type AnnouncementDepositBase = AnnouncementDepositBase;
    type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

impl pallet_utility::Config for Runtime {
    type Event = Event;
    type Call = Call;
    type PalletsOrigin = OriginCaller;
    type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

/// Local origins on this chain are allowed to dispatch XCM sends/executions. However, we later
/// block this via `ExecuteXcmOrigin`.
pub type LocalOriginToLocation = SignedToAccountId32<Origin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
    // Two routers - use UMP to communicate with the relay chain:
    cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm>,
    // ..and XCMP to communicate with the sibling chains.
    XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
    const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;

    type Origin = Origin;
    type Call = Call;
    type Event = Event;
    type SendXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
    type XcmRouter = XcmRouter;
    type ExecuteXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
    type XcmExecuteFilter = Nothing;
    type XcmReserveTransferFilter = Everything;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    // Teleporting is disabled.
    type XcmTeleportFilter = Nothing;
    type Weigher = FixedWeightBounds<BaseXcmWeight, Call, MaxInstructions>;
    type LocationInverter = LocationInverter<Ancestry>;
    type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
}

impl cumulus_pallet_xcm::Config for Runtime {
    type Event = Event;
    type XcmExecutor = XcmExecutor<XcmConfig>;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type Event = Event;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type ExecuteOverweightOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ChannelInfo = ParachainSystem;
    type VersionWrapper = PolkadotXcm;
    type ControllerOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
    type WeightInfo = weights::cumulus_pallet_xcmp_queue::WeightInfo<Runtime>;
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
    type Event = Event;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type ExecuteOverweightOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
}

parameter_types! {
    pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT / 4;
    pub const ReservedDmpWeight: Weight =  MAXIMUM_BLOCK_WEIGHT / 4;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type Event = Event;
    type OnSystemEvent = ();
    type SelfParaId = ParachainInfo;
    type DmpMessageHandler = DmpQueue;
    type OutboundXcmpMessageSource = XcmpQueue;
    type XcmpMessageHandler = XcmpQueue;
    type ReservedXcmpWeight = ReservedXcmpWeight;
    type ReservedDmpWeight = ReservedDmpWeight;
    type CheckAssociatedRelayNumber = cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;
}

impl parachain_info::Config for Runtime {}

parameter_types! {
    pub RelayLocation: MultiLocation = MultiLocation::parent();
    pub const RelayNetwork: NetworkId = NetworkId::Kusama;
    pub RelayCurrency: CurrencyId = KSM;
    pub HeikoNetwork: NetworkId = NetworkId::Named(WeakBoundedVec::<u8, ConstU32<32>>::force_from("heiko".into(), None));
    pub RelayChainOrigin: Origin = cumulus_pallet_xcm::Origin::Relay.into();
    pub Ancestry: MultiLocation = MultiLocation::new(0, X1(Parachain(ParachainInfo::parachain_id().into())));
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
    // The parent (Relay-chain) origin converts to the default `AccountId`.
    ParentIsPreset<AccountId>,
    // Sibling parachain origins convert to AccountId via the `ParaId::into`.
    SiblingParachainConvertsVia<Sibling, AccountId>,
    // Straight up local `AccountId32` origins just alias directly to `AccountId`.
    AccountId32Aliases<RelayNetwork, AccountId>,
);

parameter_types! {
    pub const NativeCurrencyId: CurrencyId = NATIVE_ASSET_ID;
    pub GiftAccount: AccountId = PalletId(*b"par/gift").into_account_truncating();
}

pub struct GiftConvert;
impl BalanceConversion<Balance, CurrencyId, Balance> for GiftConvert {
    type Error = DispatchError;
    fn to_asset_balance(balance: Balance, asset_id: CurrencyId) -> Result<Balance, Self::Error> {
        let decimal = <Assets as InspectMetadata<AccountId>>::decimals(&asset_id);
        if decimal.is_zero() {
            return Ok(Zero::zero());
        }

        let default_gift_amount = 65 * DOLLARS / 100; // 0.65HKO
        Ok(match asset_id {
            KSM if balance
                >= 10_u128
                    .pow((decimal - 1).into())
                    .saturating_sub(96_000_000u128) =>
            {
                default_gift_amount
            }
            EUSDT | EUSDC if balance >= 300 * 10_u128.pow(decimal.into()) => default_gift_amount,
            _ => Zero::zero(),
        })
    }
}

/// Means for transacting assets on this chain.
pub type LocalAssetTransactor = MultiCurrencyAdapter<
    // Use this currency:
    CurrencyAdapter,
    // Use this currency when it is a fungible asset matching the given location or name:
    IsNativeConcrete<CurrencyId, CurrencyIdConvert<AssetRegistry>>,
    // Our chain's account ID type (we can't get away without mentioning it explicitly):
    AccountId,
    Balance,
    // Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
    LocationToAccountId,
    CurrencyIdConvert<AssetRegistry>,
    NativeCurrencyId,
    ExistentialDeposit,
    GiftAccount,
    GiftConvert,
>;

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
    // Sovereign account converter; this attempts to derive an `AccountId` from the origin location
    // using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
    // foreign chains who want to have a local sovereign account on this chain which they control.
    SovereignSignedViaLocation<LocationToAccountId, Origin>,
    // Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
    // recognised.
    RelayChainAsNative<RelayChainOrigin, Origin>,
    // Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
    // recognised.
    SiblingParachainAsNative<cumulus_pallet_xcm::Origin, Origin>,
    // Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
    // transaction from the Root origin.
    ParentAsSuperuser<Origin>,
    // Native signed account converter; this just converts an `AccountId32` origin into a normal
    // `Origin::Signed` origin of the same 32-byte value.
    SignedAccountId32AsNative<RelayNetwork, Origin>,
    // Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
    XcmPassthrough<Origin>,
);

match_types! {
    pub type ParentOrSiblings: impl Contains<MultiLocation> = {
        MultiLocation { parents: 1, interior: Here } |
        MultiLocation { parents: 1, interior: X1(_) }
    };
}

pub type Barrier = (
    TakeWeightCredit,
    AllowKnownQueryResponses<PolkadotXcm>,
    AllowSubscriptionsFrom<ParentOrSiblings>,
    AllowTopLevelPaidExecutionFrom<Everything>,
);

pub struct ToTreasury;
impl TakeRevenue for ToTreasury {
    fn take_revenue(revenue: MultiAsset) {
        if let MultiAsset {
            id: AssetId::Concrete(id),
            fun: Fungibility::Fungible(amount),
        } = revenue
        {
            if let Some(currency_id) = CurrencyIdConvert::<AssetRegistry>::convert(id) {
                let _ = Assets::mint_into(currency_id, &TreasuryAccount::get(), amount);
            }
        }
    }
}

parameter_types! {
    pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

/// The non-reserve fungible transactor type
/// It will use pallet-assets, and the Id will be matched against AsAssetType
pub type ForeignFungiblesTransactor = FungiblesAdapter<
    // Use this fungibles implementation:
    Assets,
    // Use this currency when it is a fungible asset matching the given location or name:
    (
        ConvertedConcreteAssetId<
            CurrencyId,
            Balance,
            AsAssetType<CurrencyId, AssetType, AssetRegistry>,
            JustTry,
        >,
    ),
    // Do a simple punn to convert an AccountId20 MultiLocation into a native chain account ID:
    LocationToAccountId,
    // Our chain's account ID type (we can't get away without mentioning it explicitly):
    AccountId,
    // We dont allow teleports.
    Nothing,
    // We dont track any teleports
    CheckingAccount,
>;

/// How to withdraw and deposit an asset, try LocalAssetTransactor first
/// and if AssetNotFound then with ForeignFungiblesTransactor as fallback
pub type AssetTransactors = (LocalAssetTransactor, ForeignFungiblesTransactor);

/// This is the struct that will handle the revenue from xcm fees
/// We do not burn anything because we want to mimic exactly what
/// the sovereign account has
pub type XcmFeesToAccount = pallet_traits::xcm::XcmFeesToAccount<
    Assets,
    (
        ConvertedConcreteAssetId<
            CurrencyId,
            Balance,
            AsAssetType<CurrencyId, AssetType, AssetRegistry>,
            JustTry,
        >,
    ),
    AccountId,
    TreasuryAccount,
>;

pub struct XcmConfig;
impl Config for XcmConfig {
    type Call = Call;
    type XcmSender = XcmRouter;
    // How to withdraw and deposit an asset.
    type AssetTransactor = AssetTransactors;
    type OriginConverter = XcmOriginToTransactDispatchOrigin;
    type IsReserve = MultiNativeAsset<AbsoluteReserveProvider>;
    // Teleporting is disabled.
    type IsTeleporter = ();
    type LocationInverter = LocationInverter<Ancestry>;
    type Barrier = Barrier;
    type Weigher = FixedWeightBounds<BaseXcmWeight, Call, MaxInstructions>;
    type Trader = FirstAssetTrader<AssetType, AssetRegistry, XcmFeesToAccount>;
    type ResponseHandler = PolkadotXcm;
    type SubscriptionService = PolkadotXcm;
    type AssetTrap = PolkadotXcm;
    type AssetClaims = PolkadotXcm;
}

impl pallet_asset_registry::Config for Runtime {
    type Event = Event;
    type Balance = Balance;
    type AssetId = CurrencyId;
    type AssetType = AssetType;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_asset_registry::WeightInfo<Runtime>;
}

parameter_types! {
      pub const MinimumCount: u32 = 3;
      pub const ExpiresIn: Moment = 1000 * 60 * 60; // 60 mins
      pub const MaxHasDispatchedSize: u32 = 100;
      pub OneAccount: AccountId = AccountId::from([1u8; 32]);
}

type ParallelDataProvider = orml_oracle::Instance1;
impl orml_oracle::Config<ParallelDataProvider> for Runtime {
    type Event = Event;
    type OnNewData = ();
    type CombineData =
        orml_oracle::DefaultCombineData<Runtime, MinimumCount, ExpiresIn, ParallelDataProvider>;
    type Time = Timestamp;
    type OracleKey = CurrencyId;
    type OracleValue = Price;
    type RootOperatorAccountId = OneAccount;
    type MaxHasDispatchedSize = MaxHasDispatchedSize;
    type WeightInfo = weights::orml_oracle::WeightInfo<Runtime>;
    type Members = OracleMembership;
}

pub type TimeStampedPrice = orml_oracle::TimestampedValue<Price, Moment>;
pub struct AggregatedDataProvider;
impl DataProvider<CurrencyId, TimeStampedPrice> for AggregatedDataProvider {
    fn get(key: &CurrencyId) -> Option<TimeStampedPrice> {
        Oracle::get(key)
    }
}

impl DataProviderExtended<CurrencyId, TimeStampedPrice> for AggregatedDataProvider {
    fn get_no_op(key: &CurrencyId) -> Option<TimeStampedPrice> {
        Oracle::get_no_op(key)
    }

    fn get_all_values() -> Vec<(CurrencyId, Option<TimeStampedPrice>)> {
        Oracle::get_all_values()
    }
}

impl DataFeeder<CurrencyId, TimeStampedPrice, AccountId> for AggregatedDataProvider {
    fn feed_value(_: AccountId, _: CurrencyId, _: TimeStampedPrice) -> DispatchResult {
        Err("Not supported".into())
    }
}

pub struct Decimal;
impl DecimalProvider<CurrencyId> for Decimal {
    fn get_decimal(asset_id: &CurrencyId) -> Option<u8> {
        match *asset_id {
            NATIVE_ASSET_ID => Some(12_u8),
            _ => {
                let decimal = <Assets as InspectMetadata<AccountId>>::decimals(asset_id);
                if decimal.is_zero() {
                    None
                } else {
                    Some(decimal)
                }
            }
        }
    }
}

impl pallet_prices::Config for Runtime {
    type Event = Event;
    type Source = AggregatedDataProvider;
    type FeederOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type LiquidStakingExchangeRateProvider = LiquidStaking;
    type LiquidStakingCurrenciesProvider = LiquidStaking;
    type VaultTokenCurrenciesFilter = Crowdloans;
    type VaultTokenExchangeRateProvider = Crowdloans;
    type VaultLoansRateProvider = Loans;
    type RelayCurrency = RelayCurrency;
    type Decimal = Decimal;
    type AMM = AMM;
    type Assets = CurrencyAdapter;
    type WeightInfo = pallet_prices::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    // One storage item; key size is 32; value is size 4+4+16+32 bytes = 56 bytes.
    pub const DepositBase: Balance = deposit(1, 88);
    // Additional storage item size of 32 bytes.
    pub const DepositFactor: Balance = deposit(0, 32);
    pub const MaxSignatories: u16 = 100;
}

impl pallet_multisig::Config for Runtime {
    type Event = Event;
    type Call = Call;
    type Currency = Balances;
    type DepositBase = DepositBase;
    type DepositFactor = DepositFactor;
    type MaxSignatories = MaxSignatories;
    type WeightInfo = weights::pallet_multisig::WeightInfo<Runtime>;
}

parameter_types! {
    pub const BasicDeposit: Balance = deposit(1, 258);
    pub const FieldDeposit: Balance = deposit(1, 66);
    pub const SubAccountDeposit: Balance  = deposit(1, 53);
    pub const MaxSubAccounts: u32 = 100;
    pub const MaxAdditionalFields: u32 = 100;
    pub const MaxRegistrars: u32 = 20;
}

impl pallet_identity::Config for Runtime {
    type Event = Event;
    type Currency = Balances;
    type BasicDeposit = BasicDeposit;
    type FieldDeposit = FieldDeposit;
    type SubAccountDeposit = SubAccountDeposit;
    type MaxSubAccounts = MaxSubAccounts;
    type MaxAdditionalFields = MaxAdditionalFields;
    type MaxRegistrars = MaxRegistrars;
    type Slashed = Treasury;
    type ForceOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RegistrarOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_identity::WeightInfo<Runtime>;
}

type EnsureRootOrMoreThanHalfGeneralCouncil = EitherOfDiverse<
    EnsureRoot<AccountId>,
    pallet_collective::EnsureProportionMoreThan<AccountId, GeneralCouncilCollective, 1, 2>,
>;
type EnsureRootOrAllTechnicalCommittee = EitherOfDiverse<
    EnsureRoot<AccountId>,
    pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 1, 1>,
>;

parameter_types! {
    pub const LaunchPeriod: BlockNumber = 1 * DAYS;
    pub const VotingPeriod: BlockNumber = 5 * DAYS;
    pub const FastTrackVotingPeriod: BlockNumber = 3 * HOURS;
    pub const InstantAllowed: bool = true;
    pub const MinimumDeposit: Balance = 100 * DOLLARS;
    pub const EnactmentPeriod: BlockNumber = 1 * DAYS;
    pub const CooloffPeriod: BlockNumber = 7 * DAYS;
    // One cent: $10,000 / MB
    pub const MaxVotes: u32 = 100;
    pub const MaxProposals: u32 = 100;
}

impl pallet_democracy::Config for Runtime {
    type Proposal = Call;
    type Event = Event;
    type Currency = Balances;
    type EnactmentPeriod = EnactmentPeriod;
    type LaunchPeriod = LaunchPeriod;
    type VotingPeriod = VotingPeriod;
    type MinimumDeposit = MinimumDeposit;
    /// A straight majority of the council can decide what their next motion is.
    type ExternalOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, GeneralCouncilCollective, 1, 2>;
    /// A super-majority can have the next scheduled referendum be a straight majority-carries vote.
    type ExternalMajorityOrigin =
        pallet_collective::EnsureProportionMoreThan<AccountId, GeneralCouncilCollective, 1, 2>;
    /// A unanimous council can have the next scheduled referendum be a straight default-carries
    /// (NTB) vote.
    type ExternalDefaultOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, GeneralCouncilCollective, 1, 1>;
    /// Two thirds of the technical committee can have an ExternalMajority/ExternalDefault vote
    /// be tabled immediately and with a shorter voting/enactment period.
    type FastTrackOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 2, 3>;
    type InstantOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 1, 1>;
    type InstantAllowed = InstantAllowed;
    type FastTrackVotingPeriod = FastTrackVotingPeriod;
    // To cancel a proposal which has been passed, 2/3 of the council must agree to it.
    type CancellationOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, GeneralCouncilCollective, 2, 3>;
    // To cancel a proposal before it has been passed, the technical committee must be unanimous or
    // Root must agree.
    type CancelProposalOrigin = EnsureRootOrAllTechnicalCommittee;
    type BlacklistOrigin = EnsureRoot<AccountId>;
    // Any single technical committee member may veto a coming council proposal, however they can
    // only do it once and it lasts only for the cool-off period.
    type VetoOrigin = pallet_collective::EnsureMember<AccountId, TechnicalCollective>;
    type CooloffPeriod = CooloffPeriod;
    type PreimageByteDeposit = PreimageByteDeposit;
    type OperationalPreimageOrigin =
        pallet_collective::EnsureMember<AccountId, GeneralCouncilCollective>;
    type Slash = Treasury;
    type Scheduler = Scheduler;
    type PalletsOrigin = OriginCaller;
    type MaxVotes = MaxVotes;
    type WeightInfo = weights::pallet_democracy::WeightInfo<Runtime>;
    type MaxProposals = MaxProposals;
    type VoteLockingPeriod = EnactmentPeriod;
}

parameter_types! {
    pub const GeneralCouncilMotionDuration: BlockNumber = 3 * DAYS;
    pub const GeneralCouncilMaxProposals: u32 = 100;
    pub const GeneralCouncilMaxMembers: u32 = 100;
}

type GeneralCouncilCollective = pallet_collective::Instance1;
impl pallet_collective::Config<GeneralCouncilCollective> for Runtime {
    type Origin = Origin;
    type Proposal = Call;
    type Event = Event;
    type MotionDuration = GeneralCouncilMotionDuration;
    type MaxProposals = GeneralCouncilMaxProposals;
    type MaxMembers = GeneralCouncilMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

type GeneralCouncilMembershipInstance = pallet_membership::Instance1;
impl pallet_membership::Config<GeneralCouncilMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = GeneralCouncil;
    type MembershipChanged = GeneralCouncil;
    type MaxMembers = GeneralCouncilMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

parameter_types! {
    pub const TechnicalMotionDuration: BlockNumber = 3 * DAYS;
    pub const TechnicalMaxProposals: u32 = 100;
    pub const TechnicalMaxMembers: u32 = 100;
}

type TechnicalCollective = pallet_collective::Instance2;
impl pallet_collective::Config<TechnicalCollective> for Runtime {
    type Origin = Origin;
    type Proposal = Call;
    type Event = Event;
    type MotionDuration = TechnicalMotionDuration;
    type MaxProposals = TechnicalMaxProposals;
    type MaxMembers = TechnicalMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

type TechnicalCommitteeMembershipInstance = pallet_membership::Instance2;
impl pallet_membership::Config<TechnicalCommitteeMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = TechnicalCommittee;
    type MembershipChanged = TechnicalCommittee;
    type MaxMembers = TechnicalMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

parameter_types! {
    pub const PreimageMaxSize: u32 = 4096 * 1024;
    pub const PreimageBaseDeposit: Balance = deposit(2, 64);
    pub const PreimageByteDeposit: Balance = deposit(0, 1);
}

impl pallet_preimage::Config for Runtime {
    type WeightInfo = weights::pallet_preimage::WeightInfo<Runtime>;
    type Event = Event;
    type Currency = Balances;
    type ManagerOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MaxSize = PreimageMaxSize;
    type BaseDeposit = PreimageBaseDeposit;
    type ByteDeposit = PreimageByteDeposit;
}
parameter_types! {
    pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
        RuntimeBlockWeights::get().max_block;
    pub const MaxScheduledPerBlock: u32 = 50;
    pub const NoPreimagePostponement: Option<u32> = Some(10);
}

impl pallet_scheduler::Config for Runtime {
    type Event = Event;
    type Origin = Origin;
    type PalletsOrigin = OriginCaller;
    type Call = Call;
    type MaximumWeight = MaximumSchedulerWeight;
    type ScheduleOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MaxScheduledPerBlock = MaxScheduledPerBlock;
    type OriginPrivilegeCmp = EqualPrivilegeOnly;
    type WeightInfo = weights::pallet_scheduler::WeightInfo<Runtime>;
    type PreimageProvider = Preimage;
    type NoPreimagePostponement = NoPreimagePostponement;
}

parameter_types! {
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: Balance = 1 * DOLLARS;
    pub const ProposalBondMaximum: Balance = 5 * DOLLARS;
    pub const SpendPeriod: BlockNumber = 6 * DAYS;
    pub const Burn: Permill = Permill::from_percent(0);
    pub const TreasuryPalletId: PalletId = PalletId(*b"par/trsy");
    pub const MaxApprovals: u32 = 100;
}

impl pallet_treasury::Config for Runtime {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type ApproveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RejectOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type Event = Event;
    type OnSlash = ();
    type ProposalBond = ProposalBond;
    type ProposalBondMinimum = ProposalBondMinimum;
    type ProposalBondMaximum = ProposalBondMaximum;
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = ();
    type WeightInfo = weights::pallet_treasury::WeightInfo<Runtime>;
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = frame_support::traits::NeverEnsureOrigin<Balance>;
}

parameter_types! {
    pub const OracleMaxMembers: u32 = 100;
}

type OracleMembershipInstance = pallet_membership::Instance3;
impl pallet_membership::Config<OracleMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = ();
    type MembershipChanged = ();
    type MaxMembers = OracleMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

parameter_types! {
    pub const BridgeMaxMembers: u32 = 100;
}

pub struct ChangeBridgeMembers;
impl ChangeMembers<AccountId> for ChangeBridgeMembers {
    fn change_members_sorted(_incoming: &[AccountId], _outgoing: &[AccountId], new: &[AccountId]) {
        if let Err(e) = Bridge::change_vote_threshold() {
            log::error!(
                target: "bridge::change_members_sorted",
                "Failed to set vote threshold: {:?}",
                e,
            );
        } else {
            log::info!(
                target: "bridge::change_members_sorted",
                "Succeeded to set vote threshold, total members: {:?}",
                new.len(),
            );
        };
    }
}

type BridgeMembershipInstance = pallet_membership::Instance6;
impl pallet_membership::Config<BridgeMembershipInstance> for Runtime {
    type Event = Event;
    type AddOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RemoveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SwapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ResetOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type PrimeOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MembershipInitialized = ();
    type MembershipChanged = ChangeBridgeMembers;
    type MaxMembers = BridgeMaxMembers;
    type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

parameter_types! {
    pub const ParallelHeiko: ChainId = 0;
    pub const BridgePalletId: PalletId = PalletId(*b"par/brid");
    // About 30 days: 30 * 24 * 60 * 60 / 6 = 2592000 blocks
    pub const ProposalLifetime: BlockNumber = 2592000;
    pub const ThresholdPercentage: u32 = 80;
}

impl pallet_bridge::Config for Runtime {
    type Event = Event;
    type RelayMembers = BridgeMembership;
    type RootOperatorAccountId = OneAccount;
    type UpdateChainOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type UpdateTokenOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type CapOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ChainId = ParallelHeiko;
    type PalletId = BridgePalletId;
    type Assets = CurrencyAdapter;
    type GiftAccount = GiftAccount;
    type GiftConvert = GiftConvert;
    type NativeCurrencyId = NativeCurrencyId;
    type ExistentialDeposit = ExistentialDeposit;
    type ProposalLifetime = ProposalLifetime;
    type ThresholdPercentage = ThresholdPercentage;
    type WeightInfo = weights::pallet_bridge::WeightInfo<Runtime>;
}

parameter_types! {
    pub MinVestedTransfer: Balance = 0;
    pub const MaxVestingSchedules: u32 = 100;
}

impl orml_vesting::Config for Runtime {
    type Event = Event;
    type Currency = Balances;
    type MinVestedTransfer = MinVestedTransfer;
    type VestedTransferOrigin = frame_system::EnsureSigned<AccountId>;
    type WeightInfo = weights::orml_vesting::WeightInfo<Runtime>;
    type MaxVestingSchedules = MaxVestingSchedules;
    type BlockNumberProvider = frame_system::Pallet<Runtime>;
}

parameter_types! {
    pub const AMMPalletId: PalletId = PalletId(*b"par/ammp");
    pub DefaultLpFee: Ratio = Ratio::from_rational(30u32, 10000u32);        // 0.30%
    pub DefaultProtocolFee: Ratio = Ratio::from_rational(0u32, 10000u32);   // 0.00% no fees for launch
    pub DefaultProtocolFeeReceiver: AccountId = TreasuryPalletId::get().into_account_truncating();
    pub const MinimumLiquidity: u128 = 1_000u128;
}

impl pallet_amm::Config for Runtime {
    type Event = Event;
    type Assets = CurrencyAdapter;
    type PalletId = AMMPalletId;
    type LockAccountId = OneAccount;
    type CreatePoolOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type ProtocolFeeUpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type AMMWeightInfo = weights::pallet_amm::WeightInfo<Runtime>;
    type LpFee = DefaultLpFee;
    type MinimumLiquidity = MinimumLiquidity;
    type MaxLengthRoute = MaxLengthRoute;
    type GetNativeCurrencyId = NativeCurrencyId;
}

parameter_types! {
    pub const CrowdloansPalletId: PalletId = PalletId(*b"crwloans");
    pub const MinContribution: Balance = 100_000_000_000;
    pub const MigrateKeysLimit: u32 = 5;
    pub const RemoveKeysLimit: u32 = 1000;
    pub RefundLocation: AccountId = Utility::derivative_account_id(ParachainInfo::parachain_id().into_account_truncating(), u16::MAX);
    pub LeasePeriod: BlockNumber = 42 * 2 * DAYS;
    pub LeaseOffset: BlockNumber = 0;
    pub LeasePerYear: BlockNumber = 8;
}

pub struct RelayChainValidationDataProvider<T>(sp_std::marker::PhantomData<T>);

impl<T: cumulus_pallet_parachain_system::Config> BlockNumberProvider
    for RelayChainValidationDataProvider<T>
{
    type BlockNumber = primitives::BlockNumber;

    fn current_block_number() -> Self::BlockNumber {
        cumulus_pallet_parachain_system::Pallet::<T>::validation_data()
            .map(|d| d.relay_parent_number)
            .unwrap_or_default()
    }
}

impl<T: cumulus_pallet_parachain_system::Config> ValidationDataProvider
    for RelayChainValidationDataProvider<T>
{
    fn validation_data() -> Option<PersistedValidationData> {
        cumulus_pallet_parachain_system::Pallet::<T>::validation_data()
    }
}

impl pallet_crowdloans::Config for Runtime {
    type Event = Event;
    type Origin = Origin;
    type Call = Call;
    type PalletId = CrowdloansPalletId;
    type SelfParaId = ParachainInfo;
    type Assets = Assets;
    type RelayCurrency = RelayCurrency;
    type MinContribution = MinContribution;
    type MigrateKeysLimit = MigrateKeysLimit;
    type RemoveKeysLimit = RemoveKeysLimit;
    type ProxyOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type MigrateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type VrfOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type CreateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type DissolveOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type RefundOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type OpenCloseOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type AuctionSucceededFailedOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type SlotExpiredOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_crowdloans::WeightInfo<Runtime>;
    type XCM = XcmHelper;
    type RelayChainBlockNumberProvider = RelayChainValidationDataProvider<Runtime>;
    type Members = CrowdloansAutomatorsMembership;
    type LeasePeriod = LeasePeriod;
    type LeaseOffset = LeaseOffset;
    type LeasePerYear = LeasePerYear;
    type Streaming = ();
    type GetNativeCurrencyId = NativeCurrencyId;
    type Decimal = Decimal;
}

parameter_types! {
    pub const StreamPalletId: PalletId = PalletId(*b"par/strm");
    pub const MaxStreamsCount: u32 = 128;
    pub const MaxFinishedStreamsCount: u32 = 10;
}

impl pallet_streaming::Config for Runtime {
    type Event = Event;
    type Assets = CurrencyAdapter;
    type PalletId = StreamPalletId;
    type MaxStreamsCount = MaxStreamsCount;
    type MaxFinishedStreamsCount = MaxFinishedStreamsCount;
    type UnixTime = Timestamp;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_streaming::WeightInfo<Runtime>;
    type NativeCurrencyId = NativeCurrencyId;
    type NativeExistentialDeposit = ExistentialDeposit;
}

parameter_types! {
    pub const XcmHelperPalletId: PalletId = PalletId(*b"par/fees");
    pub const NotifyTimeout: BlockNumber = 100;
}

impl pallet_xcm_helper::Config for Runtime {
    type Event = Event;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type Assets = Assets;
    type XcmSender = XcmRouter;
    type RelayNetwork = RelayNetwork;
    type PalletId = XcmHelperPalletId;
    type NotifyTimeout = NotifyTimeout;
    type AccountIdToMultiLocation = AccountIdToMultiLocation<AccountId>;
    type RefundLocation = RefundLocation;
    type BlockNumberProvider = frame_system::Pallet<Runtime>;
    type WeightInfo = weights::pallet_xcm_helper::WeightInfo<Runtime>;
    type RelayCurrency = RelayCurrency;
}

parameter_types! {
    pub const MaxLengthRoute: u8 = 10;
    pub const RouterPalletId: PalletId = PalletId(*b"ammroute");
}

impl pallet_router::Config for Runtime {
    type Event = Event;
    type PalletId = RouterPalletId;
    type AMM = AMM;
    type AMMRouterWeightInfo = weights::pallet_router::WeightInfo<Runtime>;
    type MaxLengthRoute = MaxLengthRoute;
    type Assets = CurrencyAdapter;
    type GetNativeCurrencyId = NativeCurrencyId;
}

impl pallet_currency_adapter::Config for Runtime {
    type Assets = Assets;
    type Balances = Balances;
    type GetNativeCurrencyId = NativeCurrencyId;
    type LockOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
}

parameter_types! {
    pub const FarmingPalletId: PalletId = PalletId(*b"par/farm");
    pub const MaxUserLockItemsCount: u32 = 100;
    pub const LockPoolMaxDuration: u32 = 2628000;
    pub const CoolDownMaxDuration: u32 = 50400;
}

impl pallet_farming::Config for Runtime {
    type Event = Event;
    type Assets = CurrencyAdapter;
    type PalletId = FarmingPalletId;
    type UpdateOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type WeightInfo = weights::pallet_farming::WeightInfo<Runtime>;
    type MaxUserLockItemsCount = MaxUserLockItemsCount;
    type LockPoolMaxDuration = LockPoolMaxDuration;
    type CoolDownMaxDuration = CoolDownMaxDuration;
    type Decimal = Decimal;
}

impl pallet_emergency_shutdown::Config for Runtime {
    type Event = Event;
    type Whitelist = WhiteListFilter;
    type ShutdownOrigin = EnsureRootOrMoreThanHalfGeneralCouncil;
    type Call = Call;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = opaque::Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        // System, Utility, Currencies
        System: frame_system::{Pallet, Call, Storage, Config, Event<T>} = 0,
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 1,
        Utility: pallet_utility::{Pallet, Call, Event} = 2,
        Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 3,
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 4,
        TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 5,
        Assets: pallet_assets::{Pallet, Call, Storage, Event<T>} = 6,
        Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 7,
        Identity: pallet_identity::{Pallet, Call, Storage, Event<T>} = 8,

        // Governance
        Democracy: pallet_democracy::{Pallet, Call, Storage, Config<T>, Event<T>} = 11,
        GeneralCouncil: pallet_collective::<Instance1>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 12,
        TechnicalCommittee: pallet_collective::<Instance2>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 13,
        Treasury: pallet_treasury::{Pallet, Call, Storage, Config, Event<T>} = 14,
        Scheduler: pallet_scheduler::{Pallet, Call, Storage, Event<T>} = 15,
        Preimage: pallet_preimage::{Pallet, Call, Storage, Event<T>} = 16,

        // Parachain
        ParachainInfo: parachain_info::{Pallet, Storage, Config} = 21,
        XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>} = 22,
        DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>} = 23,
        PolkadotXcm: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin, Config} = 24,
        CumulusXcm: cumulus_pallet_xcm::{Pallet, Call, Event<T>, Origin} = 25,

        // Consensus
        Authorship: pallet_authorship::{Pallet, Call, Storage} = 30,
        CollatorSelection: pallet_collator_selection::{Pallet, Call, Storage, Event<T>, Config<T>} = 31,
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 32,
        Aura: pallet_aura::{Pallet, Config<T>, Storage} = 33,
        AuraExt: cumulus_pallet_aura_ext::{Pallet, Config, Storage} = 34,

        // 3rd Party
        Oracle: orml_oracle::<Instance1>::{Pallet, Storage, Call, Event<T>} = 42,
        XTokens: orml_xtokens::{Pallet, Storage, Call, Event<T>} = 43,
        OrmlXcm: orml_xcm::{Pallet, Call, Event<T>} = 45,
        Vesting: orml_vesting::{Pallet, Storage, Call, Event<T>, Config<T>} = 46,

        // Loans
        Loans: pallet_loans::{Pallet, Call, Storage, Event<T>} = 50,
        Prices: pallet_prices::{Pallet, Storage, Call, Event<T>} = 51,
        Crowdloans: pallet_crowdloans::{Pallet, Call, Storage, Event<T>} = 52,

        // LiquidStaking
        LiquidStaking: pallet_liquid_staking::{Pallet, Call, Storage, Event<T>, Config} = 60,

        // Membership
        GeneralCouncilMembership: pallet_membership::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>} = 70,
        TechnicalCommitteeMembership: pallet_membership::<Instance2>::{Pallet, Call, Storage, Event<T>, Config<T>} = 71,
        OracleMembership: pallet_membership::<Instance3>::{Pallet, Call, Storage, Event<T>, Config<T>} = 72,
        LiquidStakingAgentsMembership: pallet_membership::<Instance5>::{Pallet, Call, Storage, Event<T>, Config<T>} = 73,
        BridgeMembership: pallet_membership::<Instance6>::{Pallet, Call, Storage, Event<T>, Config<T>} = 74,
        CrowdloansAutomatorsMembership: pallet_membership::<Instance7>::{Pallet, Call, Storage, Event<T>, Config<T>} = 75,

        // AMM
        AMM: pallet_amm::{Pallet, Call, Storage, Event<T>} = 80,
        AMMRoute: pallet_router::{Pallet, Call, Event<T>} = 81,
        CurrencyAdapter: pallet_currency_adapter::{Pallet, Call} = 82,

        // Others
        Bridge: pallet_bridge::{Pallet, Call, Storage, Event<T>} = 90,
        EmergencyShutdown: pallet_emergency_shutdown::{Pallet, Call, Storage, Event<T>} = 91,
        Farming: pallet_farming::{Pallet, Call, Storage, Event<T>} = 92,
        XcmHelper: pallet_xcm_helper::{Pallet, Call, Storage, Event<T>} = 93,
        Streaming: pallet_streaming::{Pallet, Call, Storage, Event<T>} = 94,
        AssetRegistry: pallet_asset_registry::{Pallet, Call, Storage, Event<T>} = 95,

        // Parachain System, always put it at the end
        ParachainSystem: cumulus_pallet_parachain_system::{Pallet, Call, Config, Storage, Inherent, Event<T>, ValidateUnsigned} = 20,
    }
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    (),
>;

impl_runtime_apis! {
    impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
        }

        fn authorities() -> Vec<AuraId> {
            Aura::authorities().into_inner()
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
            opaque::SessionKeys::generate(seed)
        }

        fn decode_session_keys(
            encoded: Vec<u8>,
        ) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: Block) {
            Executive::execute_block(block)
        }

        fn initialize_block(header: &<Block as BlockT>::Header) {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }

        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: Block,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
        fn account_nonce(account: AccountId) -> Index {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
    }

    impl orml_oracle_rpc_runtime_api::OracleApi<
        Block,
        DataProviderId,
        CurrencyId,
        TimeStampedPrice,
    > for Runtime {
        fn get_value(provider_id: DataProviderId, key: CurrencyId) -> Option<TimeStampedPrice> {
            match provider_id {
                DataProviderId::Aggregated => Prices::get_no_op(&key)
            }
        }

        fn get_all_values(provider_id: DataProviderId) -> Vec<(CurrencyId, Option<TimeStampedPrice>)> {
            match provider_id {
                DataProviderId::Aggregated => Prices::get_all_values()
            }
        }
    }

    impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
        fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
            ParachainSystem::collect_collation_info(header)
        }
    }

    impl pallet_loans_rpc_runtime_api::LoansApi<Block, AccountId, Balance> for Runtime {
        fn get_account_liquidity(account: AccountId) -> Result<(Liquidity, Shortfall, Liquidity, Shortfall), DispatchError> {
            Loans::get_account_liquidity(&account)
        }

        fn get_market_status(asset_id: CurrencyId) -> Result<(Rate, Rate, Rate, Ratio, Balance, Balance, sp_runtime::FixedU128), DispatchError> {
            Loans::get_market_status(asset_id)
        }

        fn get_liquidation_threshold_liquidity(account: AccountId) -> Result<(Liquidity, Shortfall, Liquidity, Shortfall), DispatchError> {
            Loans::get_account_liquidation_threshold_liquidity(&account)
        }
    }

    impl pallet_router_rpc_runtime_api::RouterApi<Block, Balance> for Runtime {
        fn get_best_route(amount: Balance, token_in: CurrencyId, token_out: CurrencyId, reversed: bool) -> Result<(Vec<CurrencyId>, Balance), DispatchError> {
            let (route, amount) = AMMRoute::get_best_route(amount, token_in, token_out, reversed)?;
            Ok((route, amount))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(extra: bool) -> (
            Vec<frame_benchmarking::BenchmarkList>,
            Vec<frame_support::traits::StorageInfo>,
        ) {
            use frame_benchmarking::{list_benchmark, Benchmarking, BenchmarkList};
            use frame_support::traits::StorageInfoTrait;

            // Trying to add benchmarks directly to the Session Pallet caused cyclic dependency
            // issues. To get around that, we separated the Session benchmarks into its own crate,
            // which is why we need these two lines below.
            // use pallet_loans_benchmarking::Pallet as LoansBench;
            use frame_system_benchmarking::Pallet as SystemBench;

            let mut list = Vec::<BenchmarkList>::new();

            list_benchmark!(list, extra, pallet_balances, Balances);
            list_benchmark!(list, extra, pallet_membership, TechnicalCommitteeMembership);
            list_benchmark!(list, extra, pallet_multisig, Multisig);
            list_benchmark!(list, extra, pallet_bridge, Bridge);
            list_benchmark!(list, extra, pallet_loans, Loans);
            list_benchmark!(list, extra, frame_system, SystemBench::<Runtime>);
            list_benchmark!(list, extra, pallet_timestamp, Timestamp);
            list_benchmark!(list, extra, pallet_amm, AMM);
            list_benchmark!(list, extra, pallet_liquid_staking, LiquidStaking);
            list_benchmark!(list, extra, pallet_router, AMMRoute);
            list_benchmark!(list, extra, pallet_crowdloans, Crowdloans);
            list_benchmark!(list, extra, pallet_xcm_helper, XcmHelper);
            list_benchmark!(list, extra, pallet_farming, Farming);
            list_benchmark!(list, extra, pallet_asset_registry, AssetRegistry);
            list_benchmark!(list, extra, pallet_streaming, Streaming);
            list_benchmark!(list, extra, pallet_assets, Assets);
            list_benchmark!(list, extra, pallet_collator_selection, CollatorSelection);
            list_benchmark!(list, extra, pallet_proxy, Proxy);
            list_benchmark!(list, extra, pallet_utility, Utility);
            list_benchmark!(list, extra, cumulus_pallet_xcmp_queue, XcmpQueue);
            list_benchmark!(list, extra, pallet_identity, Identity);
            list_benchmark!(list, extra, pallet_democracy, Democracy);
            list_benchmark!(list, extra, pallet_collective, TechnicalCommittee);
            list_benchmark!(list, extra, pallet_preimage, Preimage);
            list_benchmark!(list, extra, pallet_scheduler, Scheduler);
            list_benchmark!(list, extra, pallet_treasury, Treasury);

            let storage_info = AllPalletsWithSystem::storage_info();

            (list, storage_info)
        }

        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
            use frame_benchmarking::{Benchmarking, BenchmarkBatch, add_benchmark, TrackedStorageKey};

            // use pallet_loans_benchmarking::Pallet as LoansBench;
            use frame_system_benchmarking::Pallet as SystemBench;

            // impl pallet_loans_benchmarking::Config for Runtime {}
            impl frame_system_benchmarking::Config for Runtime {}

            let whitelist: Vec<TrackedStorageKey> = vec![
                // Block Number
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
                // Total Issuance
                hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
                // Execution Phase
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
                // Event Count
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
                // System Events
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
            ];

            let mut batches = Vec::<BenchmarkBatch>::new();
            let params = (&config, &whitelist);

            add_benchmark!(params, batches, frame_system, SystemBench::<Runtime>);
            add_benchmark!(params, batches, pallet_balances, Balances);
            add_benchmark!(params, batches, pallet_timestamp, Timestamp);
            add_benchmark!(params, batches, pallet_bridge, Bridge);
            add_benchmark!(params, batches, pallet_loans, Loans);
            add_benchmark!(params, batches, pallet_multisig, Multisig);
            add_benchmark!(params, batches, pallet_membership, TechnicalCommitteeMembership);
            add_benchmark!(params, batches, pallet_amm, AMM);
            add_benchmark!(params, batches, pallet_liquid_staking, LiquidStaking);
            add_benchmark!(params, batches, pallet_router, AMMRoute);
            add_benchmark!(params, batches, pallet_crowdloans, Crowdloans);
            add_benchmark!(params, batches, pallet_xcm_helper, XcmHelper);
            add_benchmark!(params, batches, pallet_farming, Farming);
            add_benchmark!(params, batches, pallet_asset_registry, AssetRegistry);
            add_benchmark!(params, batches, pallet_streaming, Streaming);
            add_benchmark!(params, batches, pallet_assets, Assets);
            add_benchmark!(params, batches, pallet_collator_selection, CollatorSelection);
            add_benchmark!(params, batches, pallet_proxy, Proxy);
            add_benchmark!(params, batches, pallet_utility, Utility);
            add_benchmark!(params, batches, cumulus_pallet_xcmp_queue, XcmpQueue);
            add_benchmark!(params, batches, pallet_identity, Identity);
            add_benchmark!(params, batches, pallet_democracy, Democracy);
            add_benchmark!(params, batches, pallet_collective, TechnicalCommittee);
            add_benchmark!(params, batches, pallet_preimage, Preimage);
            add_benchmark!(params, batches, pallet_scheduler, Scheduler);
            add_benchmark!(params, batches, pallet_treasury, Treasury);

            if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
            Ok(batches)
        }
    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade() -> (Weight, Weight) {
            log::info!("try-runtime::on_runtime_upgrade.");
            let weight = Executive::try_runtime_upgrade().unwrap();
            (weight, RuntimeBlockWeights::get().max_block)
        }

        fn execute_block_no_check(block: Block) -> Weight {
            Executive::execute_block_no_check(block)
        }
    }
}

struct CheckInherents;

impl cumulus_pallet_parachain_system::CheckInherents<Block> for CheckInherents {
    fn check_inherents(
        block: &Block,
        relay_state_proof: &cumulus_pallet_parachain_system::RelayChainStateProof,
    ) -> sp_inherents::CheckInherentsResult {
        let relay_chain_slot = relay_state_proof
            .read_slot()
            .expect("Could not read the relay chain slot from the proof");

        let inherent_data =
            cumulus_primitives_timestamp::InherentDataProvider::from_relay_chain_slot_and_duration(
                relay_chain_slot,
                sp_std::time::Duration::from_secs(6),
            )
            .create_inherent_data()
            .expect("Could not create the timestamp inherent data");

        inherent_data.check_extrinsics(block)
    }
}

cumulus_pallet_parachain_system::register_validate_block!(
    Runtime = Runtime,
    BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
    CheckInherents = CheckInherents,
);
