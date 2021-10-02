use anchor_lang::{
    prelude::*,
    solana_program::{entrypoint::ProgramResult, program::invoke_signed, program_pack::Pack},
};
use spl_token::{instruction, state};
pub use spl_token::{state::AccountState as TokenAccountState, ID};
use std::{io::Write, ops::Deref};

#[derive(Debug, Clone, Copy)]
pub struct SplToken;

impl anchor_lang::AccountDeserialize for SplToken {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Self::try_deserialize_unchecked(buf)
    }

    fn try_deserialize_unchecked(_buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Ok(Self)
    }
}

impl anchor_lang::Id for SplToken {
    fn id() -> Pubkey {
        ID
    }
}

#[derive(Clone)]
pub struct TokenMint(state::Mint);

impl anchor_lang::AccountDeserialize for TokenMint {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Self::try_deserialize_unchecked(buf)
    }

    fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        state::Mint::unpack(buf).map(Self)
    }
}

impl anchor_lang::AccountSerialize for TokenMint {
    fn try_serialize<W: Write>(&self, _writer: &mut W) -> Result<(), ProgramError> {
        Ok(())
    }
}

impl anchor_lang::Owner for TokenMint {
    fn owner() -> Pubkey {
        ID
    }
}

impl Deref for TokenMint {
    type Target = state::Mint;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct TokenAccount(state::Account);

impl anchor_lang::AccountDeserialize for TokenAccount {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Self::try_deserialize_unchecked(buf)
    }

    fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        state::Account::unpack(buf).map(Self)
    }
}

impl anchor_lang::AccountSerialize for TokenAccount {
    fn try_serialize<W: Write>(&self, _writer: &mut W) -> Result<(), ProgramError> {
        Ok(())
    }
}

impl anchor_lang::Owner for TokenAccount {
    fn owner() -> Pubkey {
        ID
    }
}

impl Deref for TokenAccount {
    type Target = state::Account;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn transfer<'info>(
    from: AccountInfo<'info>,
    to: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    seeds: &[&[&[u8]]],
) -> ProgramResult {
    invoke_signed(
        &instruction::transfer(&ID, from.key, to.key, authority.key, &[], amount)?,
        &[from, to, authority],
        seeds,
    )
}
