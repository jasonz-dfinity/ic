//! State management module.
//!
//! The state is stored in the global thread-level variable `__STATE`.
//! This module provides utility functions to manage the state. Most
//! code should use those functions instead of touching `__STATE` directly.
#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

pub mod audit;
pub mod eventlog;
pub mod invariants;

use crate::lifecycle::init::InitArgs;
use crate::lifecycle::upgrade::UpgradeArgs;
use crate::logs::P0;
use crate::state::invariants::{CheckInvariants, CheckInvariantsImpl};
use crate::updates::update_balance::SuspendedUtxo;
use crate::{
    address::BitcoinAddress, compute_min_withdrawal_amount, ECDSAPublicKey, GetUtxosCache, Network,
    Timestamp,
};
use candid::{CandidType, Deserialize, Principal};
use ic_base_types::CanisterId;
use ic_btc_interface::{MillisatoshiPerByte, OutPoint, Txid, Utxo};
use ic_canister_log::log;
use ic_utils_ensure::ensure_eq;
use icrc_ledger_types::icrc1::account::Account;
use serde::Serialize;
use std::collections::btree_map::Entry;
use std::collections::btree_set;
use std::iter::Chain;
use std::time::Duration;

/// The maximum number of finalized BTC retrieval requests that we keep in the
/// history.
const MAX_FINALIZED_REQUESTS: usize = 100;

thread_local! {
    static __STATE: RefCell<Option<CkBtcMinterState>> = RefCell::default();
}

// A pending retrieve btc request
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize, candid::CandidType)]
pub struct RetrieveBtcRequest {
    /// The amount to convert to BTC.
    /// The minter withdraws BTC transfer fees from this amount.
    pub amount: u64,
    /// The destination BTC address.
    pub address: BitcoinAddress,
    /// The BURN transaction index on the ledger.
    /// Serves as a unique request identifier.
    pub block_index: u64,
    /// The time at which the minter accepted the request.
    pub received_at: u64,
    /// The KYT provider that validated this request.
    /// The field is optional because old retrieve_btc requests
    /// didn't go through the KYT check.
    #[serde(rename = "kyt_provider")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kyt_provider: Option<Principal>,
    /// The reimbursement_account of the retrieve_btc transaction.
    #[serde(rename = "reimbursement_account")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reimbursement_account: Option<Account>,
}

/// A transaction output storing the minter's change.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize, candid::CandidType)]
pub struct ChangeOutput {
    /// The index of the output in the transaction.
    pub vout: u32,
    /// The value of the output.
    pub value: u64,
}

/// Represents a transaction sent to the Bitcoin network.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
pub struct SubmittedBtcTransaction {
    /// The original retrieve_btc requests that initiated the transaction.
    pub requests: Vec<RetrieveBtcRequest>,
    /// The identifier of the unconfirmed transaction.
    pub txid: Txid,
    /// The list of UTXOs we used in the transaction.
    pub used_utxos: Vec<Utxo>,
    /// The IC time at which we submitted the Bitcoin transaction.
    pub submitted_at: u64,
    /// The tx output from the submitted transaction that the minter owns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_output: Option<ChangeOutput>,
    /// Fee per vbyte in millisatoshi.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_per_vbyte: Option<u64>,
}

/// Pairs a retrieve_btc request with its outcome.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
pub struct FinalizedBtcRetrieval {
    /// The original retrieve_btc request that initiated the transaction.
    pub request: RetrieveBtcRequest,
    /// The state of the finalized request.
    pub state: FinalizedStatus,
}

/// The outcome of a retrieve_btc request.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
pub enum FinalizedStatus {
    /// The request amount was to low to cover the fees.
    AmountTooLow,
    /// The transaction that retrieves BTC got enough confirmations.
    Confirmed {
        /// The witness transaction identifier of the transaction.
        txid: Txid,
    },
}

/// The status of a Bitcoin transaction that the minter hasn't yet sent to the Bitcoin network.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
pub enum InFlightStatus {
    /// Awaiting signatures for transaction inputs.
    Signing,
    /// Awaiting the Bitcoin canister to accept the transaction.
    Sending { txid: Txid },
}

/// The status of a retrieve_btc request.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, candid::CandidType)]
pub enum RetrieveBtcStatus {
    /// The minter has no data for this request.
    /// The request id is either invalid or too old.
    Unknown,
    /// The request is in the batch queue.
    Pending,
    /// Waiting for a signature on a transaction satisfy this request.
    Signing,
    /// Sending the transaction satisfying this request.
    Sending { txid: Txid },
    /// Awaiting for confirmations on the transaction satisfying this request.
    Submitted { txid: Txid },
    /// The retrieval amount was too low. Satisfying the request is impossible.
    AmountTooLow,
    /// Confirmed a transaction satisfying this request.
    Confirmed { txid: Txid },
}

#[derive(Clone, Eq, PartialEq, Debug, CandidType, Deserialize)]
pub struct BtcRetrievalStatusV2 {
    pub block_index: u64,
    pub status_v2: Option<RetrieveBtcStatusV2>,
}

impl From<RetrieveBtcStatus> for RetrieveBtcStatusV2 {
    fn from(status: RetrieveBtcStatus) -> Self {
        match status {
            RetrieveBtcStatus::Unknown => RetrieveBtcStatusV2::Unknown,
            RetrieveBtcStatus::Pending => RetrieveBtcStatusV2::Pending,
            RetrieveBtcStatus::Signing => RetrieveBtcStatusV2::Signing,
            RetrieveBtcStatus::Sending { txid } => RetrieveBtcStatusV2::Sending { txid },
            RetrieveBtcStatus::Submitted { txid } => RetrieveBtcStatusV2::Submitted { txid },
            RetrieveBtcStatus::AmountTooLow => RetrieveBtcStatusV2::AmountTooLow,
            RetrieveBtcStatus::Confirmed { txid } => RetrieveBtcStatusV2::Confirmed { txid },
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, CandidType, Deserialize)]
pub enum RetrieveBtcStatusV2 {
    /// The minter has no data for this request.
    /// The request id is either invalid or too old.
    Unknown,
    /// The request is in the batch queue.
    Pending,
    /// Waiting for a signature on a transaction satisfy this request.
    Signing,
    /// Sending the transaction satisfying this request.
    Sending { txid: Txid },
    /// Awaiting for confirmations on the transaction satisfying this request.
    Submitted { txid: Txid },
    /// The retrieval amount was too low. Satisfying the request is impossible.
    AmountTooLow,
    /// Confirmed a transaction satisfying this request.
    Confirmed { txid: Txid },
    /// The retrieve bitcoin request has been reimbursed.
    Reimbursed(ReimbursedDeposit),
    /// The minter will try to reimburse this transaction.
    WillReimburse(ReimburseDepositTask),
}

/// Controls which operations the minter can perform.
#[derive(Clone, Eq, PartialEq, Debug, Serialize, candid::CandidType, serde::Deserialize)]
pub enum Mode {
    /// Minter's state is read-only.
    ReadOnly,
    /// Only the specified principals can modify the minter's state.
    RestrictedTo(Vec<Principal>),
    /// Only the specified principals can deposit BTC.
    DepositsRestrictedTo(Vec<Principal>),
    /// No restrictions on the minter interactions.
    GeneralAvailability,
}

impl Mode {
    /// Returns Ok if the specified principal can convert BTC to ckBTC.
    pub fn is_deposit_available_for(&self, p: &Principal) -> Result<(), String> {
        match self {
            Self::GeneralAvailability => Ok(()),
            Self::ReadOnly => Err("the minter is in read-only mode".to_string()),
            Self::RestrictedTo(allow_list) => {
                if !allow_list.contains(p) {
                    return Err("access to the minter is temporarily restricted".to_string());
                }
                Ok(())
            }
            Self::DepositsRestrictedTo(allow_list) => {
                if !allow_list.contains(p) {
                    return Err("BTC deposits are temporarily restricted".to_string());
                }
                Ok(())
            }
        }
    }

    /// Returns Ok if the specified principal can convert ckBTC to BTC.
    pub fn is_withdrawal_available_for(&self, p: &Principal) -> Result<(), String> {
        match self {
            Self::GeneralAvailability | Self::DepositsRestrictedTo(_) => Ok(()),
            Self::ReadOnly => Err("the minter is in read-only mode".to_string()),
            Self::RestrictedTo(allow_list) => {
                if !allow_list.contains(p) {
                    return Err("BTC withdrawals are temporarily restricted".to_string());
                }
                Ok(())
            }
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::GeneralAvailability
    }
}

/// The outcome of a UTXO check.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum UtxoCheckStatus {
    /// The Bitcoin check did not reveal any problems.
    Clean,
    /// The UTXO in question is tainted.
    Tainted,
    /// The UTXO is clean but minting failed.
    CleanButMintUnknown,
}

/// Relevant data for a checked UTXO. The UUID and `kyt_provider` are kept for
/// backward-compatibility reasons. They should be set to `None` since
/// we dont use KYT providers anymore.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CheckedUtxo {
    pub(crate) status: UtxoCheckStatus,
    uuid: Option<String>,
    kyt_provider: Option<Principal>,
}

/// Indicates that fee distribution overdrafted.
#[derive(Copy, Clone, Debug)]
pub struct Overdraft(pub u64);

/// The state of the ckBTC Minter.
///
/// Every piece of state of the Minter should be stored as field of this struct.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CkBtcMinterState {
    /// The Bitcoin network that the minter will connect to
    pub btc_network: Network,

    /// The name of the [EcdsaKeyId]. Use "dfx_test_key" for local replica and "test_key_1" for
    /// a testing key for testnet and mainnet
    pub ecdsa_key_name: String,

    /// The Minter ECDSA public key
    pub ecdsa_public_key: Option<ECDSAPublicKey>,

    /// The minimum number of confirmations on the Bitcoin chain.
    pub min_confirmations: u32,

    /// Maximum time of nanoseconds that a transaction should spend in the queue
    /// before being sent.
    pub max_time_in_queue_nanos: u64,

    /// Per-account lock for update_balance
    pub update_balance_accounts: BTreeSet<Account>,

    /// Per-account lock for retrieve_btc
    pub retrieve_btc_accounts: BTreeSet<Account>,

    /// Minimum amount of bitcoin that can be retrieved
    pub retrieve_btc_min_amount: u64,

    /// Minimum amount of bitcoin that can be retrieved based on recent fees
    pub fee_based_retrieve_btc_min_amount: u64,

    /// Retrieve_btc requests that are waiting to be served, sorted by
    /// received_at.
    pub pending_retrieve_btc_requests: Vec<RetrieveBtcRequest>,

    /// Maps Account to its retrieve_btc requests burn block indices.
    pub retrieve_btc_account_to_block_indices: BTreeMap<Account, Vec<u64>>,

    /// The identifiers of retrieve_btc requests which we're currently signing a
    /// transaction or sending to the Bitcoin network.
    pub requests_in_flight: BTreeMap<u64, InFlightStatus>,

    /// Last transaction submission timestamp.
    pub last_transaction_submission_time_ns: Option<u64>,

    /// BTC transactions waiting for finalization.
    pub submitted_transactions: Vec<SubmittedBtcTransaction>,

    /// Transactions that likely didn't make it into the mempool.
    pub stuck_transactions: Vec<SubmittedBtcTransaction>,

    /// Maps ID of a stuck transaction to the ID of the corresponding replacement transaction.
    pub replacement_txid: BTreeMap<Txid, Txid>,
    /// Maps ID of a replacement transaction to the ID of the corresponding stuck transaction.
    pub rev_replacement_txid: BTreeMap<Txid, Txid>,

    /// Finalized retrieve_btc requests for which we received enough confirmations.
    pub finalized_requests: VecDeque<FinalizedBtcRetrieval>,

    /// The total number of finalized requests.
    pub finalized_requests_count: u64,

    /// The total amount of ckBTC minted.
    pub tokens_minted: u64,

    /// The total amount of ckBTC burned.
    pub tokens_burned: u64,

    /// The CanisterId of the ckBTC Ledger.
    pub ledger_id: CanisterId,

    /// The principal of the Bitcoin checker canister.
    pub btc_checker_principal: Option<CanisterId>,

    /// The set of UTXOs unused in pending transactions.
    pub available_utxos: BTreeSet<Utxo>,

    /// The mapping from output points to the ledger accounts to which they
    /// belong.
    pub outpoint_account: BTreeMap<OutPoint, Account>,

    /// The map of known addresses to their utxos.
    pub utxos_state_addresses: BTreeMap<Account, BTreeSet<Utxo>>,

    /// This map contains the UTXOs we removed due to a transaction finalization
    /// while there was a concurrent update_balance call for the account whose
    /// UTXOs participated in the transaction.
    ///
    /// We insert a new entry into this map if we discover a concurrent
    /// update_balance calls during a transaction finalization and remove the
    /// entry once the update_balance call completes.
    pub finalized_utxos: BTreeMap<Account, BTreeSet<Utxo>>,

    /// Process one timer event at a time.
    pub is_timer_running: bool,

    pub is_distributing_fee: bool,

    /// The mode in which the minter runs.
    pub mode: Mode,

    pub last_fee_per_vbyte: Vec<u64>,

    /// The fee for a single Bitcoin check request.
    pub check_fee: u64,

    /// The total amount of fees we owe to the KYT provider.
    pub owed_kyt_amount: BTreeMap<Principal, u64>,

    /// A cache of UTXO check statuses.
    pub checked_utxos: BTreeMap<Utxo, CheckedUtxo>,

    /// UTXOs that cannot be yet processed.
    pub suspended_utxos: SuspendedUtxos,

    /// Map from burn block index to amount to reimburse because of
    /// check fees.
    pub pending_reimbursements: BTreeMap<u64, ReimburseDepositTask>,

    /// Map from burn block index to the the reimbursed request.
    pub reimbursed_transactions: BTreeMap<u64, ReimbursedDeposit>,

    /// Cache of get_utxos call results
    pub get_utxos_cache: GetUtxosCache,
}

#[derive(Clone, Eq, PartialEq, Debug, CandidType, Serialize, serde::Deserialize)]
pub struct ReimburseDepositTask {
    pub account: Account,
    pub amount: u64,
    pub reason: ReimbursementReason,
}

#[derive(Clone, Eq, PartialEq, Debug, CandidType, Serialize, serde::Deserialize)]
pub struct ReimbursedDeposit {
    pub account: Account,
    pub amount: u64,
    pub reason: ReimbursementReason,
    pub mint_block_index: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize, Serialize, candid::CandidType)]
pub enum ReimbursementReason {
    TaintedDestination {
        kyt_provider: Principal,
        kyt_fee: u64,
    },
    CallFailed,
}

impl CkBtcMinterState {
    #[allow(deprecated)]
    pub fn reinit(
        &mut self,
        InitArgs {
            btc_network,
            ecdsa_key_name,
            retrieve_btc_min_amount,
            ledger_id,
            max_time_in_queue_nanos,
            min_confirmations,
            mode,
            check_fee,
            btc_checker_principal,
            kyt_principal: _,
            kyt_fee,
            get_utxos_cache_expiration_seconds,
        }: InitArgs,
    ) {
        self.btc_network = btc_network;
        self.ecdsa_key_name = ecdsa_key_name;
        self.retrieve_btc_min_amount = retrieve_btc_min_amount;
        self.fee_based_retrieve_btc_min_amount = retrieve_btc_min_amount;
        self.ledger_id = ledger_id;
        self.max_time_in_queue_nanos = max_time_in_queue_nanos;
        self.mode = mode;
        self.btc_checker_principal = btc_checker_principal;
        if let Some(check_fee) = check_fee {
            self.check_fee = check_fee;
        } else if let Some(kyt_fee) = kyt_fee {
            self.check_fee = kyt_fee;
        }
        if let Some(min_confirmations) = min_confirmations {
            self.min_confirmations = min_confirmations;
        }
        if let Some(expiration) = get_utxos_cache_expiration_seconds {
            self.get_utxos_cache
                .set_expiration(Duration::from_secs(expiration));
        }
    }

    #[allow(deprecated)]
    pub fn upgrade(
        &mut self,
        UpgradeArgs {
            retrieve_btc_min_amount,
            max_time_in_queue_nanos,
            min_confirmations,
            mode,
            check_fee,
            btc_checker_principal,
            kyt_principal: _,
            kyt_fee,
            get_utxos_cache_expiration_seconds,
        }: UpgradeArgs,
    ) {
        if let Some(retrieve_btc_min_amount) = retrieve_btc_min_amount {
            self.retrieve_btc_min_amount = retrieve_btc_min_amount;
            self.fee_based_retrieve_btc_min_amount = retrieve_btc_min_amount;
        }
        if let Some(max_time_in_queue_nanos) = max_time_in_queue_nanos {
            self.max_time_in_queue_nanos = max_time_in_queue_nanos;
        }
        if let Some(min_conf) = min_confirmations {
            if min_conf < self.min_confirmations {
                self.min_confirmations = min_conf;
            } else {
                log!(
                    P0,
                    "Didn't increase min_confirmations to {} (current value: {})",
                    min_conf,
                    self.min_confirmations
                );
            }
        }
        if let Some(mode) = mode {
            self.mode = mode;
        }
        if let Some(btc_checker_principal) = btc_checker_principal {
            self.btc_checker_principal = Some(btc_checker_principal);
        }
        if let Some(check_fee) = check_fee {
            self.check_fee = check_fee;
        } else if let Some(kyt_fee) = kyt_fee {
            self.check_fee = kyt_fee;
        }
        if let Some(expiration) = get_utxos_cache_expiration_seconds {
            self.get_utxos_cache
                .set_expiration(Duration::from_secs(expiration));
        }
    }

    pub fn validate_config(&self) {
        if self.check_fee > self.retrieve_btc_min_amount {
            ic_cdk::trap("check_fee cannot be greater than retrieve_btc_min_amount");
        }
        if self.ecdsa_key_name.is_empty() {
            ic_cdk::trap("ecdsa_key_name is not set");
        }
        if self.btc_checker_principal.is_none() {
            ic_cdk::trap("Bitcoin checker principal is not set");
        }
    }

    pub fn check_invariants(&self) -> Result<(), String> {
        CheckInvariantsImpl::check_invariants(self)
    }

    // public for only for tests
    pub(crate) fn add_utxos<I: CheckInvariants>(&mut self, account: Account, utxos: Vec<Utxo>) {
        if utxos.is_empty() {
            return;
        }

        self.tokens_minted += utxos.iter().map(|u| u.value).sum::<u64>();

        let account_bucket = self.utxos_state_addresses.entry(account).or_default();

        for utxo in utxos {
            self.outpoint_account.insert(utxo.outpoint.clone(), account);
            self.available_utxos.insert(utxo.clone());
            self.checked_utxos.remove(&utxo);
            account_bucket.insert(utxo);
        }

        if cfg!(debug_assertions) {
            I::check_invariants(self).expect("state invariants are violated");
        }
    }

    pub fn retrieve_btc_status_v2_by_account(
        &self,
        target: Option<Account>,
    ) -> Vec<BtcRetrievalStatusV2> {
        let target_account = target.unwrap_or(Account {
            owner: ic_cdk::caller(),
            subaccount: None,
        });

        let block_indices: Vec<u64> = self
            .retrieve_btc_account_to_block_indices
            .get(&target_account)
            .unwrap_or(&vec![])
            .to_vec();

        let result: Vec<BtcRetrievalStatusV2> = block_indices
            .iter()
            .map(|&block_index| BtcRetrievalStatusV2 {
                block_index,
                status_v2: Some(self.retrieve_btc_status_v2(block_index)),
            })
            .collect();

        result
    }

    pub fn retrieve_btc_status_v2(&self, block_index: u64) -> RetrieveBtcStatusV2 {
        if let Some(reimbursement) = self.pending_reimbursements.get(&block_index) {
            return RetrieveBtcStatusV2::WillReimburse(reimbursement.clone());
        }

        if let Some(reimbursement) = self.reimbursed_transactions.get(&block_index) {
            return RetrieveBtcStatusV2::Reimbursed(reimbursement.clone());
        }

        let status_v2: RetrieveBtcStatusV2 = self.retrieve_btc_status(block_index).into();

        status_v2
    }

    /// Returns the status of the retrieve_btc request with the specified
    /// identifier.
    pub fn retrieve_btc_status(&self, block_index: u64) -> RetrieveBtcStatus {
        if self
            .pending_retrieve_btc_requests
            .iter()
            .any(|req| req.block_index == block_index)
        {
            return RetrieveBtcStatus::Pending;
        }

        if let Some(status) = self.requests_in_flight.get(&block_index).cloned() {
            return match status {
                InFlightStatus::Signing => RetrieveBtcStatus::Signing,
                InFlightStatus::Sending { txid } => RetrieveBtcStatus::Sending { txid },
            };
        }

        if let Some(txid) = self.submitted_transactions.iter().find_map(|tx| {
            (tx.requests.iter().any(|r| r.block_index == block_index)).then_some(tx.txid)
        }) {
            return RetrieveBtcStatus::Submitted { txid };
        }

        match self
            .finalized_requests
            .iter()
            .find(|finalized_request| finalized_request.request.block_index == block_index)
            .map(|final_req| final_req.state.clone())
        {
            Some(FinalizedStatus::AmountTooLow) => return RetrieveBtcStatus::AmountTooLow,
            Some(FinalizedStatus::Confirmed { txid }) => {
                return RetrieveBtcStatus::Confirmed { txid }
            }
            None => (),
        }

        RetrieveBtcStatus::Unknown
    }

    /// Returns true if the pending requests queue has enough requests to form a
    /// batch or there are old enough requests to form a batch.
    pub fn can_form_a_batch(&self, min_pending: usize, now: u64) -> bool {
        if self.pending_retrieve_btc_requests.len() >= min_pending {
            return true;
        }

        if let Some(req) = self.pending_retrieve_btc_requests.first() {
            if self.max_time_in_queue_nanos < now.saturating_sub(req.received_at) {
                return true;
            }
        }

        if let Some(req) = self.pending_retrieve_btc_requests.last() {
            if let Some(last_submission_time) = self.last_transaction_submission_time_ns {
                if self.max_time_in_queue_nanos
                    < req.received_at.saturating_sub(last_submission_time)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Forms a batch of retrieve_btc requests that the minter can fulfill.
    pub fn build_batch(&mut self, max_size: usize) -> Vec<RetrieveBtcRequest> {
        let available_utxos_value = self.available_utxos.iter().map(|u| u.value).sum::<u64>();
        let mut batch = vec![];
        let mut tx_amount = 0;
        for req in std::mem::take(&mut self.pending_retrieve_btc_requests) {
            if available_utxos_value < req.amount + tx_amount || batch.len() >= max_size {
                // Put this request back to the queue until we have enough liquid UTXOs.
                self.pending_retrieve_btc_requests.push(req);
            } else {
                tx_amount += req.amount;
                batch.push(req);
            }
        }

        batch
    }

    /// Returns the total number of all retrieve_btc requests that we haven't
    /// finalized yet.
    pub fn count_incomplete_retrieve_btc_requests(&self) -> usize {
        self.pending_retrieve_btc_requests.len()
            + self.requests_in_flight.len()
            + self
                .submitted_transactions
                .iter()
                .map(|tx| tx.requests.len())
                .sum::<usize>()
    }

    /// Returns true if there is a pending retrieve_btc request with the given
    /// identifier.
    fn has_pending_request(&self, block_index: u64) -> bool {
        self.pending_retrieve_btc_requests
            .iter()
            .any(|req| req.block_index == block_index)
    }

    fn forget_utxo(&mut self, utxo: &Utxo) {
        if let Some(account) = self.outpoint_account.remove(&utxo.outpoint) {
            if self.update_balance_accounts.contains(&account) {
                self.finalized_utxos
                    .entry(account)
                    .or_default()
                    .insert(utxo.clone());
            }

            let last_utxo = match self.utxos_state_addresses.get_mut(&account) {
                Some(utxo_set) => {
                    utxo_set.remove(utxo);
                    utxo_set.is_empty()
                }
                None => false,
            };
            if last_utxo {
                self.utxos_state_addresses.remove(&account);
            }
        }
    }

    pub(crate) fn finalize_transaction(&mut self, txid: &Txid) {
        let finalized_tx = if let Some(pos) = self
            .submitted_transactions
            .iter()
            .position(|tx| &tx.txid == txid)
        {
            self.submitted_transactions.swap_remove(pos)
        } else if let Some(pos) = self
            .stuck_transactions
            .iter()
            .position(|tx| &tx.txid == txid)
        {
            self.stuck_transactions.swap_remove(pos)
        } else {
            ic_cdk::trap(&format!(
                "Attempted to finalized a non-existent transaction {}",
                txid
            ));
        };

        for utxo in finalized_tx.used_utxos.iter() {
            self.forget_utxo(utxo);
        }
        self.finalized_requests_count += finalized_tx.requests.len() as u64;
        for request in finalized_tx.requests {
            self.push_finalized_request(FinalizedBtcRetrieval {
                request,
                state: FinalizedStatus::Confirmed { txid: *txid },
            });
        }

        self.cleanup_tx_replacement_chain(txid);
    }

    fn cleanup_tx_replacement_chain(&mut self, confirmed_txid: &Txid) {
        let mut txids_to_remove = BTreeSet::new();

        // Collect transactions preceding the confirmed transaction.
        let mut to_edge = *confirmed_txid;
        while let Some(from_edge) = self.replacement_txid.remove(&to_edge) {
            debug_assert_eq!(self.rev_replacement_txid.get(&from_edge), Some(&to_edge));
            self.rev_replacement_txid.remove(&from_edge);
            txids_to_remove.insert(from_edge);
            to_edge = from_edge;
        }

        // Collect transactions replacing the confirmed transaction.
        let mut from_edge = *confirmed_txid;
        while let Some(to_edge) = self.rev_replacement_txid.remove(&from_edge) {
            debug_assert_eq!(self.replacement_txid.get(&to_edge), Some(&from_edge));
            txids_to_remove.insert(to_edge);
            from_edge = to_edge;
        }

        for txid in &txids_to_remove {
            self.replacement_txid.remove(txid);
            self.rev_replacement_txid.remove(txid);
        }

        if txids_to_remove.is_empty() {
            return;
        }

        self.submitted_transactions
            .retain(|tx| !txids_to_remove.contains(&tx.txid));
        self.stuck_transactions
            .retain(|tx| !txids_to_remove.contains(&tx.txid));
    }

    pub(crate) fn longest_resubmission_chain_size(&self) -> usize {
        self.submitted_transactions
            .iter()
            .map(|tx| {
                let mut txid = &tx.txid;
                let mut len = 0;
                while let Some(older_txid) = self.rev_replacement_txid.get(txid) {
                    len += 1;
                    txid = older_txid;
                }
                len
            })
            .max()
            .unwrap_or_default()
    }

    /// Replaces a stuck transaction with a newly sent transaction.
    pub(crate) fn replace_transaction(&mut self, old_txid: &Txid, mut tx: SubmittedBtcTransaction) {
        assert_ne!(old_txid, &tx.txid);
        assert_eq!(
            self.replacement_txid.get(old_txid),
            None,
            "replacing the same transaction twice is not allowed"
        );
        for req in tx.requests.iter() {
            assert!(!self.has_pending_request(req.block_index));
        }

        let new_txid = tx.txid;
        let pos = self
            .submitted_transactions
            .iter()
            .position(|tx| &tx.txid == old_txid)
            .expect("BUG: attempted to replace an unknown transaction");

        std::mem::swap(&mut self.submitted_transactions[pos], &mut tx);
        // tx points to the old transaction now.
        debug_assert_eq!(&tx.txid, old_txid);

        self.stuck_transactions.push(tx);
        self.replacement_txid.insert(*old_txid, new_txid);
        self.rev_replacement_txid.insert(new_txid, *old_txid);
    }

    /// Returns the identifier of the most recent replacement transaction for the given stuck
    /// transaction id.
    pub fn find_last_replacement_tx(&self, txid: &Txid) -> Option<&Txid> {
        let mut last = self.replacement_txid.get(txid)?;
        while let Some(newer_txid) = self.replacement_txid.get(last) {
            last = newer_txid;
        }
        Some(last)
    }

    /// Removes a pending retrieve_btc request with the specified block index.
    fn remove_pending_request(&mut self, block_index: u64) -> Option<RetrieveBtcRequest> {
        match self
            .pending_retrieve_btc_requests
            .iter()
            .position(|req| req.block_index == block_index)
        {
            Some(pos) => Some(self.pending_retrieve_btc_requests.remove(pos)),
            None => None,
        }
    }

    /// Marks the specified retrieve_btc request as in-flight.
    ///
    /// # Panics
    ///
    /// This function panics if there is a pending retrieve_btc request with the
    /// same identifier.
    pub fn push_in_flight_request(&mut self, block_index: u64, status: InFlightStatus) {
        assert!(!self.has_pending_request(block_index));

        self.requests_in_flight.insert(block_index, status);
    }

    /// Returns a retrieve_btc requests back to the pending queue.
    ///
    /// # Panics
    ///
    /// This function panics if there is a pending retrieve_btc request with the
    /// same identifier.
    pub fn push_from_in_flight_to_pending_requests(
        &mut self,
        mut requests: Vec<RetrieveBtcRequest>,
    ) {
        for req in requests.iter() {
            assert!(!self.has_pending_request(req.block_index));
            self.requests_in_flight.remove(&req.block_index);
        }
        self.pending_retrieve_btc_requests.append(&mut requests);
        self.pending_retrieve_btc_requests
            .sort_by_key(|r| r.received_at);
    }

    /// Push back a retrieve_btc request to the ordered queue.
    ///
    /// # Panics
    ///
    /// This function panics if the new request breaks the request ordering in
    /// the queue.
    pub fn push_back_pending_request(&mut self, request: RetrieveBtcRequest) {
        if let Some(last_req) = self.pending_retrieve_btc_requests.last() {
            assert!(last_req.received_at <= request.received_at);
        }
        self.tokens_burned += request.amount;
        if let Some(kyt_provider) = request.kyt_provider {
            *self.owed_kyt_amount.entry(kyt_provider).or_insert(0) += self.check_fee;
        }
        self.pending_retrieve_btc_requests.push(request);
    }

    /// Records a BTC transaction as submitted and updates statuses of all
    /// requests involved.
    ///
    /// # Panics
    ///
    /// This function panics if there is a pending retrieve_btc request with the
    /// same identifier as one of the request used for the transaction.
    pub fn push_submitted_transaction(&mut self, tx: SubmittedBtcTransaction) {
        for req in tx.requests.iter() {
            assert!(!self.has_pending_request(req.block_index));
            self.requests_in_flight.remove(&req.block_index);
        }
        self.submitted_transactions.push(tx);
    }

    /// Marks the specified retrieve_btc request as finalized.
    ///
    /// # Panics
    ///
    /// This function panics if there is a pending retrieve_btc request with the
    /// same identifier.
    fn push_finalized_request(&mut self, req: FinalizedBtcRetrieval) {
        assert!(!self.has_pending_request(req.request.block_index));

        if self.finalized_requests.len() >= MAX_FINALIZED_REQUESTS {
            self.finalized_requests.pop_front();
        }
        self.finalized_requests.push_back(req)
    }

    /// Return UTXOs of the given account that are known to the minter.
    pub fn known_utxos_for_account(&self, account: &Account) -> Vec<Utxo> {
        let maybe_existing_utxos = self.utxos_state_addresses.get(account);
        let maybe_finalized_utxos = self.finalized_utxos.get(account);
        match (maybe_existing_utxos, maybe_finalized_utxos) {
            (Some(existing_utxos), Some(finalized_utxos)) => existing_utxos
                .union(finalized_utxos)
                .cloned()
                .collect::<Vec<_>>(),
            (Some(existing_utxos), None) => existing_utxos.iter().cloned().collect::<Vec<_>>(),
            (None, Some(finalized_utxos)) => finalized_utxos.iter().cloned().collect::<Vec<_>>(),
            (None, None) => vec![],
        }
    }

    /// Returns the UTXOs that can be processed for the given account.
    ///
    /// The returned UTXOs include:
    /// * new UTXOs that are not known to the minter,
    /// * UTXOs that were previously ignored and that can be re-evaluated,
    /// * UTXOs that were previously quarantined and that can be re-evaluated.
    pub fn processable_utxos_for_account<I: IntoIterator<Item = Utxo>>(
        &self,
        all_utxos_for_account: I,
        account: &Account,
        now: &Timestamp,
    ) -> (ProcessableUtxos, Vec<SuspendedUtxo>) {
        const DAY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

        let is_known = |utxo: &Utxo| {
            self.utxos_state_addresses
                .get(account)
                .map(|utxos| utxos.contains(utxo))
                .unwrap_or(false)
                || self
                    .finalized_utxos
                    .get(account)
                    .map(|utxos| utxos.contains(utxo))
                    .unwrap_or(false)
        };
        let mut processable_utxos = ProcessableUtxos::default();
        let mut suspended_utxos = vec![];

        for utxo in all_utxos_for_account.into_iter() {
            match self.suspended_utxos.contains_utxo(&utxo, account) {
                (Some(last_time_checked), Some(reason)) => {
                    match now.checked_duration_since(*last_time_checked) {
                        Some(elapsed) if elapsed >= DAY => {
                            processable_utxos.insert_once_suspended_utxo(utxo, reason);
                        }
                        _ => suspended_utxos.push(SuspendedUtxo {
                            utxo,
                            reason: *reason,
                            earliest_retry: last_time_checked
                                .saturating_add(DAY)
                                .as_nanos_since_unix_epoch(),
                        }),
                    }
                }
                (None, Some(reason)) => {
                    processable_utxos.insert_once_suspended_utxo(utxo, reason);
                }
                (_, None) => {
                    if !is_known(&utxo) {
                        processable_utxos.insert_once_new_utxo(utxo);
                    }
                }
            }
        }

        (processable_utxos, suspended_utxos)
    }

    /// Adds given UTXO to the set of suspended UTXOs.
    pub fn suspend_utxo(
        &mut self,
        utxo: Utxo,
        account: Account,
        reason: SuspendedReason,
        now: Timestamp,
    ) -> bool {
        self.ensure_reason_consistent_with_state(&utxo, reason);
        self.suspended_utxos
            .insert(account, utxo, reason, Some(now))
    }

    #[deprecated(note = "Use discard_utxo() instead")]
    pub fn discard_utxo_without_account(&mut self, utxo: Utxo, reason: SuspendedReason) {
        self.ensure_reason_consistent_with_state(&utxo, reason);
        #[allow(deprecated)]
        self.suspended_utxos.insert_without_account(utxo, reason)
    }

    fn ensure_reason_consistent_with_state(&self, utxo: &Utxo, reason: SuspendedReason) {
        match reason {
            SuspendedReason::ValueTooSmall => {
                assert!(utxo.value <= self.check_fee);
            }
            SuspendedReason::Quarantined => {}
        }
    }

    /// Marks the given UTXO as successfully checked.
    /// We increase the owed KYT amount if there is a KYT provider, and
    /// remember that UTXO until we see it again in an [add_utxos] call.
    #[deprecated(note = "Use mark_utxo_checked_v2() instead")]
    fn mark_utxo_checked(
        &mut self,
        utxo: Utxo,
        uuid: Option<String>,
        kyt_provider: Option<Principal>,
    ) {
        #[allow(deprecated)]
        self.suspended_utxos.remove_without_account(&utxo);
        if self
            .checked_utxos
            .insert(
                utxo,
                CheckedUtxo {
                    uuid,
                    status: UtxoCheckStatus::Clean,
                    kyt_provider,
                },
            )
            .is_none()
        {
            // Updated the owed amount only if it's the first time we mark this UTXO as
            // clean.
            if let Some(provider) = kyt_provider {
                *self.owed_kyt_amount.entry(provider).or_insert(0) += self.check_fee;
            }
        }
    }

    /// Marks the given UTXO as successfully checked but minting failed.
    fn mark_utxo_checked_mint_unknown(&mut self, utxo: Utxo, account: &Account) {
        // It should have already been removed from suspended_utxos
        debug_assert_eq!(
            self.suspended_utxos.contains_utxo(&utxo, account),
            (None, None),
            "BUG: UTXO was still suspended and cannot be marked as mint unknown"
        );
        self.checked_utxos.insert(
            utxo,
            CheckedUtxo {
                uuid: None,
                status: UtxoCheckStatus::CleanButMintUnknown,
                kyt_provider: None,
            },
        );
    }

    /// Marks the given UTXO as successfully checked.
    fn mark_utxo_checked_v2(&mut self, utxo: Utxo, account: &Account) {
        self.suspended_utxos.remove(account, &utxo);
        self.checked_utxos.insert(
            utxo,
            CheckedUtxo {
                uuid: None,
                status: UtxoCheckStatus::Clean,
                kyt_provider: None,
            },
        );
    }

    /// Decreases the owed amount for the given provider by the amount.
    /// Returns an error if the distributed amount exceeds the amount owed to the provider.
    ///
    /// NOTE: The owed balance decreases even in the case of an overdraft.
    /// That's because we mint tokens on the ledger before calling this method, so preserving the
    /// original owed amount does not make sense.
    fn distribute_kyt_fee(&mut self, provider: Principal, amount: u64) -> Result<(), Overdraft> {
        if amount == 0 {
            return Ok(());
        }
        match self.owed_kyt_amount.entry(provider) {
            Entry::Occupied(mut entry) => {
                let balance = *entry.get();
                if amount > balance {
                    entry.remove();
                    Err(Overdraft(amount - balance))
                } else {
                    *entry.get_mut() -= amount;
                    if *entry.get() == 0 {
                        entry.remove();
                    }
                    Ok(())
                }
            }
            Entry::Vacant(_) => Err(Overdraft(amount)),
        }
    }

    pub fn schedule_deposit_reimbursement(
        &mut self,
        burn_block_index: u64,
        reimburse_deposit_task: ReimburseDepositTask,
    ) {
        match reimburse_deposit_task.reason {
            ReimbursementReason::TaintedDestination {
                kyt_provider,
                kyt_fee,
            } => {
                if kyt_fee > 0 {
                    *self.owed_kyt_amount.entry(kyt_provider).or_insert(0) += kyt_fee;
                }
            }
            ReimbursementReason::CallFailed => {}
        }
        self.retrieve_btc_account_to_block_indices
            .entry(reimburse_deposit_task.account)
            .and_modify(|entry| entry.push(burn_block_index))
            .or_insert(vec![burn_block_index]);

        self.pending_reimbursements
            .insert(burn_block_index, reimburse_deposit_task);
    }

    /// Checks whether the internal state of the minter matches the other state
    /// semantically (the state holds the same data, but maybe in a slightly
    /// different form).
    pub fn check_semantically_eq(&self, other: &Self) -> Result<(), String> {
        ensure_eq!(
            self.btc_network,
            other.btc_network,
            "btc_network does not match"
        );
        ensure_eq!(
            self.ecdsa_key_name,
            other.ecdsa_key_name,
            "ecdsa_key_name does not match"
        );
        ensure_eq!(
            self.min_confirmations,
            other.min_confirmations,
            "min_confirmations does not match"
        );
        ensure_eq!(self.ledger_id, other.ledger_id, "ledger_id does not match");
        ensure_eq!(
            self.finalized_requests,
            other.finalized_requests,
            "finalized_requests do not match"
        );
        ensure_eq!(
            self.requests_in_flight,
            other.requests_in_flight,
            "requests_in_flight do not match"
        );
        ensure_eq!(
            self.available_utxos,
            other.available_utxos,
            "available_utxos do not match"
        );
        ensure_eq!(
            self.utxos_state_addresses,
            other.utxos_state_addresses,
            "utxos_state_addresses do not match"
        );
        {
            let SuspendedUtxos {
                utxos_without_account,
                utxos,
                last_time_checked_cache: _,
            } = &self.suspended_utxos;
            let SuspendedUtxos {
                utxos_without_account: other_utxos_without_account,
                utxos: other_utxos,
                last_time_checked_cache: _,
            } = &other.suspended_utxos;
            // last_time_checked_cache are not preserved on upgrades
            // to avoid adding an event every time a suspended UTXO is re-evaluated with the same outcome.
            ensure_eq!(
                utxos_without_account,
                other_utxos_without_account,
                "suspended_utxos::utxos_without_account does not match"
            );
            ensure_eq!(utxos, other_utxos, "suspended_utxos::utxos does not match");
        }

        ensure_eq!(
            self.checked_utxos,
            other.checked_utxos,
            "checked_utxos do not match"
        );

        ensure_eq!(self.check_fee, other.check_fee, "check_fee does not match");

        ensure_eq!(
            self.owed_kyt_amount,
            other.owed_kyt_amount,
            "owed_kyt_amount does not match"
        );

        ensure_eq!(
            self.btc_checker_principal,
            other.btc_checker_principal,
            "btc_checker_principal does not match"
        );

        ensure_eq!(
            self.retrieve_btc_account_to_block_indices,
            other.retrieve_btc_account_to_block_indices,
            "retrieve_btc_account_to_block_indices does not match"
        );

        let my_txs = as_sorted_vec(self.submitted_transactions.iter().cloned(), |tx| tx.txid);
        let other_txs = as_sorted_vec(other.submitted_transactions.iter().cloned(), |tx| tx.txid);
        ensure_eq!(my_txs, other_txs, "submitted_transactions do not match");

        ensure_eq!(
            self.stuck_transactions,
            other.stuck_transactions,
            "stuck_transactions do not match"
        );

        let my_requests = as_sorted_vec(self.pending_retrieve_btc_requests.iter().cloned(), |r| {
            r.block_index
        });
        let other_requests =
            as_sorted_vec(other.pending_retrieve_btc_requests.iter().cloned(), |r| {
                r.block_index
            });
        ensure_eq!(
            my_requests,
            other_requests,
            "pending_retrieve_btc_requests do not match"
        );

        ensure_eq!(
            self.replacement_txid,
            other.replacement_txid,
            "replacement_txid maps do not match"
        );

        ensure_eq!(
            self.rev_replacement_txid,
            other.rev_replacement_txid,
            "rev_replacement_txid maps do not match"
        );

        Ok(())
    }

    pub fn get_total_btc_managed(&self) -> u64 {
        let mut total_btc = 0_u64;
        for req in self.submitted_transactions.iter() {
            if let Some(change_output) = &req.change_output {
                total_btc += change_output.value;
            }
        }
        total_btc += self.available_utxos.iter().map(|u| u.value).sum::<u64>();
        total_btc
    }

    pub fn ignored_utxos(&self) -> impl Iterator<Item = &Utxo> {
        self.suspended_utxos.iter().filter_map(|(u, r)| match r {
            SuspendedReason::ValueTooSmall => Some(u),
            SuspendedReason::Quarantined => None,
        })
    }

    pub fn quarantined_utxos(&self) -> impl Iterator<Item = &Utxo> {
        self.suspended_utxos.iter().filter_map(|(u, r)| match r {
            SuspendedReason::ValueTooSmall => None,
            SuspendedReason::Quarantined => Some(u),
        })
    }

    pub fn mint_status_unknown_utxos(&self) -> impl Iterator<Item = &Utxo> {
        self.checked_utxos.iter().filter_map(|(utxo, checked)| {
            if checked.status == UtxoCheckStatus::CleanButMintUnknown {
                Some(utxo)
            } else {
                None
            }
        })
    }

    pub fn estimate_median_fee_per_vbyte(&self) -> Option<MillisatoshiPerByte> {
        /// The default fee we use on regtest networks.
        const DEFAULT_REGTEST_FEE: MillisatoshiPerByte = 5_000;

        let median_fee = match &self.btc_network {
            Network::Mainnet | Network::Testnet => {
                if self.last_fee_per_vbyte.len() < 100 {
                    return None;
                }
                Some(self.last_fee_per_vbyte[50])
            }
            Network::Regtest => Some(DEFAULT_REGTEST_FEE),
        };
        median_fee.map(|f| f.max(self.minimum_fee_per_vbyte()))
    }

    pub fn update_median_fee_per_vbyte(
        &mut self,
        fees: Vec<MillisatoshiPerByte>,
    ) -> Option<MillisatoshiPerByte> {
        if fees.len() < 100 {
            log!(
                P0,
                "[update_median_fee_per_vbyte]: not enough data points ({}) to compute the fee",
                fees.len()
            );
            return None;
        }
        self.last_fee_per_vbyte = fees;
        let median_fee = self
            .estimate_median_fee_per_vbyte()
            .expect("BUG: last_fee_per_vbyte set");
        self.fee_based_retrieve_btc_min_amount =
            compute_min_withdrawal_amount(median_fee, self.retrieve_btc_min_amount, self.check_fee);
        Some(median_fee)
    }

    /// An estimated fee per vbyte of 142 millistatoshis per vbyte was selected around 2025.06.21 01:09:50 UTC
    /// for Bitcoin Mainnet, whereas the median fee around that time should have been 2_000.
    /// Until we know the root cause, we ensure that the estimated fee has a meaningful minimum value.
    pub const fn minimum_fee_per_vbyte(&self) -> MillisatoshiPerByte {
        match &self.btc_network {
            Network::Mainnet => 1_500,
            Network::Testnet => 1_000,
            Network::Regtest => 0,
        }
    }
}

#[derive(Eq, PartialEq, Debug, Default)]
pub struct ProcessableUtxos {
    new_utxos: BTreeSet<Utxo>,
    previously_ignored_utxos: BTreeSet<Utxo>,
    previously_quarantined_utxos: BTreeSet<Utxo>,
}

impl ProcessableUtxos {
    pub fn iter(&self) -> impl Iterator<Item = &Utxo> {
        self.new_utxos
            .iter()
            .chain(&self.previously_ignored_utxos)
            .chain(&self.previously_quarantined_utxos)
    }
}

impl IntoIterator for ProcessableUtxos {
    type Item = Utxo;
    type IntoIter = Chain<
        Chain<btree_set::IntoIter<Utxo>, btree_set::IntoIter<Utxo>>,
        btree_set::IntoIter<Utxo>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.new_utxos
            .into_iter()
            .chain(self.previously_ignored_utxos)
            .chain(self.previously_quarantined_utxos)
    }
}

impl ProcessableUtxos {
    pub fn insert_once_suspended_utxo(&mut self, utxo: Utxo, reason: &SuspendedReason) {
        self.assert_utxo_is_fresh(&utxo);
        match reason {
            SuspendedReason::ValueTooSmall => self.previously_ignored_utxos.insert(utxo),
            SuspendedReason::Quarantined => self.previously_quarantined_utxos.insert(utxo),
        };
    }

    pub fn insert_once_new_utxo(&mut self, utxo: Utxo) {
        self.assert_utxo_is_fresh(&utxo);
        self.new_utxos.insert(utxo);
    }

    fn assert_utxo_is_fresh(&self, utxo: &Utxo) {
        assert!(
            !self.new_utxos.contains(utxo),
            "BUG: UTXO is already known in new_utxos"
        );
        assert!(
            !self.previously_quarantined_utxos.contains(utxo),
            "BUG: UTXO is already known in previously_quarantined_utxos"
        );
        assert!(
            !self.previously_ignored_utxos.contains(utxo),
            "BUG: UTXO is already known in previously_ignored_utxos"
        );
    }
}

#[derive(Eq, Clone, PartialEq, Debug, Default)]
pub struct SuspendedUtxos {
    /// Suspended UTXOS were initially stored without account information.
    /// A suspended UTXO is periodically reevaluated when the user calls `update_balance`,
    /// which will remove it from this data structure if the UTXO is no longer to be suspended,
    /// or move it to the other field containing this time the `Account` information.
    utxos_without_account: BTreeMap<Utxo, SuspendedReason>,
    utxos: BTreeMap<Account, BTreeMap<Utxo, SuspendedReason>>,
    last_time_checked_cache: BTreeMap<Utxo, Timestamp>,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, CandidType, Serialize, Deserialize)]
pub enum SuspendedReason {
    /// UTXO whose value is too small to pay the Bitcoin check fee.
    ValueTooSmall,
    /// UTXO that the Bitcoin checker considered tainted.
    Quarantined,
}

impl SuspendedUtxos {
    pub fn insert(
        &mut self,
        account: Account,
        utxo: Utxo,
        reason: SuspendedReason,
        now: Option<Timestamp>,
    ) -> bool {
        if let Some(timestamp) = now {
            self.last_time_checked_cache.insert(utxo.clone(), timestamp);
        }
        if self.utxos.get(&account).and_then(|u| u.get(&utxo)) == Some(&reason) {
            return false;
        }
        self.utxos_without_account.remove(&utxo);
        let utxos = self.utxos.entry(account).or_default();
        utxos.insert(utxo, reason);
        true
    }

    #[deprecated(note = "Use insert() instead")]
    pub fn insert_without_account(&mut self, utxo: Utxo, reason: SuspendedReason) {
        debug_assert!(self.utxos.values().all(|utxos| !utxos.contains_key(&utxo)));
        self.utxos_without_account.insert(utxo, reason);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Utxo, &SuspendedReason)> {
        self.utxos_without_account
            .iter()
            .chain(self.utxos.values().flat_map(|v| v.iter()))
    }

    pub fn contains_utxo(
        &self,
        utxo: &Utxo,
        account: &Account,
    ) -> (Option<&Timestamp>, Option<&SuspendedReason>) {
        let last_time_checked = self.last_time_checked_cache.get(utxo);
        let suspended_reason = self
            .utxos
            .get(account)
            .and_then(|u| u.get(utxo))
            .or_else(|| self.utxos_without_account.get(utxo));
        (last_time_checked, suspended_reason)
    }

    pub fn remove(&mut self, account: &Account, utxo: &Utxo) {
        self.last_time_checked_cache.remove(utxo);
        self.utxos_without_account.remove(utxo);
        if let Some(utxos) = self.utxos.get_mut(account) {
            utxos.remove(utxo);
        }
    }

    #[deprecated(note = "Use remove() instead")]
    pub fn remove_without_account(&mut self, utxo: &Utxo) {
        self.last_time_checked_cache.remove(utxo);
        self.utxos_without_account.remove(utxo);
        for utxos in self.utxos.values_mut() {
            if utxos.remove(utxo).is_some() {
                return; //UTXO can belong to at most one account
            }
        }
    }

    /// Number of suspended UTXOs
    pub fn num_utxos(&self) -> usize {
        self.utxos_without_account.len() + self.utxos.values().map(|u| u.len()).sum::<usize>()
    }

    pub fn utxos_without_account(&self) -> &BTreeMap<Utxo, SuspendedReason> {
        &self.utxos_without_account
    }
}

fn as_sorted_vec<T, K: Ord>(values: impl Iterator<Item = T>, key: impl Fn(&T) -> K) -> Vec<T> {
    let mut v: Vec<_> = values.collect();
    v.sort_by_key(key);
    v
}

impl From<InitArgs> for CkBtcMinterState {
    #[allow(deprecated)]
    fn from(args: InitArgs) -> Self {
        Self {
            btc_network: args.btc_network,
            ecdsa_key_name: args.ecdsa_key_name,
            ecdsa_public_key: None,
            min_confirmations: args
                .min_confirmations
                .unwrap_or(crate::lifecycle::init::DEFAULT_MIN_CONFIRMATIONS),
            max_time_in_queue_nanos: args.max_time_in_queue_nanos,
            update_balance_accounts: Default::default(),
            retrieve_btc_accounts: Default::default(),
            retrieve_btc_min_amount: args.retrieve_btc_min_amount,
            fee_based_retrieve_btc_min_amount: args.retrieve_btc_min_amount,
            pending_retrieve_btc_requests: Default::default(),
            requests_in_flight: Default::default(),
            last_transaction_submission_time_ns: None,
            submitted_transactions: Default::default(),
            replacement_txid: Default::default(),
            retrieve_btc_account_to_block_indices: Default::default(),
            rev_replacement_txid: Default::default(),
            stuck_transactions: Default::default(),
            finalized_requests: VecDeque::with_capacity(MAX_FINALIZED_REQUESTS),
            finalized_requests_count: 0,
            tokens_minted: 0,
            tokens_burned: 0,
            ledger_id: args.ledger_id,
            btc_checker_principal: args.btc_checker_principal,
            available_utxos: Default::default(),
            outpoint_account: Default::default(),
            utxos_state_addresses: Default::default(),
            finalized_utxos: Default::default(),
            is_timer_running: false,
            is_distributing_fee: false,
            mode: args.mode,
            last_fee_per_vbyte: vec![1; 100],
            check_fee: args
                .check_fee
                .unwrap_or(crate::lifecycle::init::DEFAULT_CHECK_FEE),
            owed_kyt_amount: Default::default(),
            checked_utxos: Default::default(),
            suspended_utxos: Default::default(),
            pending_reimbursements: Default::default(),
            reimbursed_transactions: Default::default(),
            get_utxos_cache: GetUtxosCache::new(Duration::from_secs(
                args.get_utxos_cache_expiration_seconds.unwrap_or_default(),
            )),
        }
    }
}

/// Take the current state.
///
/// After calling this function the state won't be initialized anymore.
/// Panics if there is no state.
pub fn take_state<F, R>(f: F) -> R
where
    F: FnOnce(CkBtcMinterState) -> R,
{
    __STATE.with(|s| f(s.take().expect("State not initialized!")))
}

/// Mutates (part of) the current state using `f`.
///
/// Panics if there is no state.
pub fn mutate_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut CkBtcMinterState) -> R,
{
    __STATE.with(|s| f(s.borrow_mut().as_mut().expect("State not initialized!")))
}

/// Read (part of) the current state using `f`.
///
/// Panics if there is no state.
pub fn read_state<F, R>(f: F) -> R
where
    F: FnOnce(&CkBtcMinterState) -> R,
{
    __STATE.with(|s| f(s.borrow().as_ref().expect("State not initialized!")))
}

/// Replaces the current state.
pub fn replace_state(state: CkBtcMinterState) {
    __STATE.with(|s| {
        *s.borrow_mut() = Some(state);
    });
}
