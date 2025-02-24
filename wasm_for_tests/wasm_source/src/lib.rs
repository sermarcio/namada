/// A tx that doesn't do anything.
#[cfg(feature = "tx_no_op")]
pub mod main {
    use namada_tx_prelude::*;

    #[transaction]
    fn apply_tx(_ctx: &mut Ctx, _tx_data: Vec<u8>) -> TxResult {
        Ok(())
    }
}

/// A tx that allocates a memory of size given from the `tx_data: usize`.
#[cfg(feature = "tx_memory_limit")]
pub mod main {
    use namada_tx_prelude::*;

    #[transaction]
    fn apply_tx(_ctx: &mut Ctx, tx_data: Vec<u8>) -> TxResult {
        let len = usize::try_from_slice(&tx_data[..]).unwrap();
        log_string(format!("allocate len {}", len));
        let bytes: Vec<u8> = vec![6_u8; len];
        // use the variable to prevent it from compiler optimizing it away
        log_string(format!("{:?}", &bytes[..8]));
        Ok(())
    }
}

/// A tx to be used as proposal_code
#[cfg(feature = "tx_proposal_code")]
pub mod main {
    use namada_tx_prelude::*;

    #[transaction]
    fn apply_tx(ctx: &mut Ctx, _tx_data: Vec<u8>) -> TxResult {
        // governance
        let target_key = gov_storage::get_min_proposal_grace_epoch_key();
        ctx.write(&target_key, 9_u64)?;

        // parameters
        let target_key = parameters_storage::get_tx_whitelist_storage_key();
        ctx.write(&target_key, vec!["hash"])?;
        Ok(())
    }
}

/// A tx that attempts to read the given key from storage.
#[cfg(feature = "tx_read_storage_key")]
pub mod main {
    use namada_tx_prelude::*;

    #[transaction]
    fn apply_tx(ctx: &mut Ctx, tx_data: Vec<u8>) -> TxResult {
        // Allocates a memory of size given from the `tx_data (usize)`
        let key = storage::Key::try_from_slice(&tx_data[..]).unwrap();
        log_string(format!("key {}", key));
        let _result: Vec<u8> = ctx.read(&key)?.unwrap();
        Ok(())
    }
}

/// A tx that attempts to write arbitrary data to the given key
#[cfg(feature = "tx_write")]
pub mod main {
    use borsh::BorshDeserialize;
    use namada_test_utils::tx_data::TxWriteData;
    use namada_tx_prelude::{
        log_string, transaction, Ctx, ResultExt, SignedTxData, StorageRead,
        StorageWrite, TxResult,
    };

    const TX_NAME: &str = "tx_write";

    fn log(msg: &str) {
        log_string(format!("[{}] {}", TX_NAME, msg))
    }

    fn fatal(msg: &str, err: impl std::error::Error) -> ! {
        log(&format!("ERROR: {} - {:?}", msg, err));
        panic!()
    }

    fn fatal_msg(msg: &str) -> ! {
        log(msg);
        panic!()
    }

    #[transaction]
    fn apply_tx(ctx: &mut Ctx, tx_data: Vec<u8>) -> TxResult {
        let signed = SignedTxData::try_from_slice(&tx_data[..])
            .wrap_err("failed to decode SignedTxData")?;
        let data = match signed.data {
            Some(data) => {
                log(&format!("got data ({} bytes)", data.len()));
                data
            }
            None => {
                fatal_msg("no data provided");
            }
        };
        let TxWriteData { key, value } =
            match TxWriteData::try_from_slice(&data[..]) {
                Ok(write_op) => {
                    log(&format!(
                        "parsed WriteOp to key {} ({} bytes)",
                        &write_op.key,
                        &write_op.value.len(),
                    ));
                    write_op
                }
                Err(error) => fatal("deserializing WriteOp", error),
            };
        let existing_value: Option<String> = ctx.read(&key)?;
        match existing_value {
            Some(existing_value) => {
                log(&format!("already present value is {}", existing_value));
            }
            None => {
                log("no already present value");
            }
        }
        log(&format!("attempting to write new value to key {}", key));
        // using `ctx.write_bytes` instead of `ctx.write` here, as we want to
        // write the actual bytes, not a Borsh-serialization of a `Vec<u8>`
        ctx.write_bytes(&key, &value[..])?;
        Ok(())
    }
}

/// A tx that attempts to mint tokens in the transfer's target without debiting
/// the tokens from the source. This tx is expected to be rejected by the
/// token's VP.
#[cfg(feature = "tx_mint_tokens")]
pub mod main {
    use namada_tx_prelude::*;

    #[transaction]
    fn apply_tx(ctx: &mut Ctx, tx_data: Vec<u8>) -> TxResult {
        let signed = SignedTxData::try_from_slice(&tx_data[..])
            .wrap_err("failed to decode SignedTxData")?;
        let transfer =
            token::Transfer::try_from_slice(&signed.data.unwrap()[..]).unwrap();
        log_string(format!("apply_tx called to mint tokens: {:#?}", transfer));
        let token::Transfer {
            source: _,
            target,
            token,
            sub_prefix: _,
            amount,
            key: _,
            shielded: _,
        } = transfer;
        let target_key = token::balance_key(&token, &target);
        let mut target_bal: token::Amount =
            ctx.read(&target_key)?.unwrap_or_default();
        target_bal.receive(&amount);
        ctx.write(&target_key, target_bal)?;
        Ok(())
    }
}

/// A VP that always returns `true`.
#[cfg(feature = "vp_always_true")]
pub mod main {
    use namada_vp_prelude::*;

    #[validity_predicate]
    fn validate_tx(
        _ctx: &Ctx,
        _tx_data: Vec<u8>,
        _addr: Address,
        _keys_changed: BTreeSet<storage::Key>,
        _verifiers: BTreeSet<Address>,
    ) -> VpResult {
        accept()
    }
}

/// A VP that always returns `false`.
#[cfg(feature = "vp_always_false")]
pub mod main {
    use namada_vp_prelude::*;

    #[validity_predicate]
    fn validate_tx(
        _ctx: &Ctx,
        _tx_data: Vec<u8>,
        _addr: Address,
        _keys_changed: BTreeSet<storage::Key>,
        _verifiers: BTreeSet<Address>,
    ) -> VpResult {
        reject()
    }
}

/// A VP that runs the VP given in `tx_data` via `eval`. It returns the result
/// of `eval`.
#[cfg(feature = "vp_eval")]
pub mod main {
    use namada_vp_prelude::*;

    #[validity_predicate]
    fn validate_tx(
        ctx: &Ctx,
        tx_data: Vec<u8>,
        _addr: Address,
        _keys_changed: BTreeSet<storage::Key>,
        _verifiers: BTreeSet<Address>,
    ) -> VpResult {
        use validity_predicate::EvalVp;
        let EvalVp { vp_code, input }: EvalVp =
            EvalVp::try_from_slice(&tx_data[..]).unwrap();
        ctx.eval(vp_code, input)
    }
}

// A VP that allocates a memory of size given from the `tx_data: usize`.
// Returns `true`, if the allocation is within memory limits.
#[cfg(feature = "vp_memory_limit")]
pub mod main {
    use namada_vp_prelude::*;

    #[validity_predicate]
    fn validate_tx(
        _ctx: &Ctx,
        tx_data: Vec<u8>,
        _addr: Address,
        _keys_changed: BTreeSet<storage::Key>,
        _verifiers: BTreeSet<Address>,
    ) -> VpResult {
        let len = usize::try_from_slice(&tx_data[..]).unwrap();
        log_string(format!("allocate len {}", len));
        let bytes: Vec<u8> = vec![6_u8; len];
        // use the variable to prevent it from compiler optimizing it away
        log_string(format!("{:?}", &bytes[..8]));
        accept()
    }
}

/// A VP that attempts to read the given key from storage (state prior to tx
/// execution). Returns `true`, if the allocation is within memory limits.
#[cfg(feature = "vp_read_storage_key")]
pub mod main {
    use namada_vp_prelude::*;

    #[validity_predicate]
    fn validate_tx(
        ctx: &Ctx,
        tx_data: Vec<u8>,
        _addr: Address,
        _keys_changed: BTreeSet<storage::Key>,
        _verifiers: BTreeSet<Address>,
    ) -> VpResult {
        // Allocates a memory of size given from the `tx_data (usize)`
        let key = storage::Key::try_from_slice(&tx_data[..]).unwrap();
        log_string(format!("key {}", key));
        let _result: Vec<u8> = ctx.read_pre(&key)?.unwrap();
        accept()
    }
}
