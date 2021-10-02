use anchor_lang::prelude::*;
use serum_dex::{matching::Side, state::MarketState};
use solana_program::program::invoke_signed;
use std::convert::TryFrom;

#[cfg(feature = "devnet")]
declare_id!("DESVgJVGajEgKGXhb6XmqDHGz3VjdgP7rEVESBgxmroY");
#[cfg(not(feature = "devnet"))]
declare_id!("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin");

#[derive(Debug, Clone, Copy)]
pub struct Dex;

impl anchor_lang::AccountDeserialize for Dex {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Self::try_deserialize_unchecked(buf)
    }

    fn try_deserialize_unchecked(_buf: &mut &[u8]) -> Result<Self, ProgramError> {
        Ok(Self)
    }
}

impl anchor_lang::Id for Dex {
    fn id() -> Pubkey {
        ID
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SideAnchor(Side);

impl AnchorDeserialize for SideAnchor {
    fn deserialize(buf: &mut &[u8]) -> Result<Self, std::io::Error> {
        let number: u8 = AnchorDeserialize::deserialize(buf)?;
        match Side::try_from(number) {
            Ok(side) => Ok(Self(side)),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No discriminant in enum matches the value",
            )),
        }
    }
}

impl AnchorSerialize for SideAnchor {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        let number: u8 = self.0.into();
        number.serialize(writer)
    }
}

impl From<SideAnchor> for Side {
    fn from(side: SideAnchor) -> Self {
        side.0
    }
}

impl From<Side> for SideAnchor {
    fn from(side: Side) -> Self {
        SideAnchor(side)
    }
}

pub fn get_market_coin_lot_size(market: &AccountInfo<'_>) -> Result<u64, ProgramError> {
    Ok(MarketState::load(market, &ID)?.coin_lot_size)
}

pub fn init_open_orders<'info>(
    dex_program: AccountInfo<'info>,
    open_orders: AccountInfo<'info>,
    owner: AccountInfo<'info>,
    market: AccountInfo<'info>,
    seeds: &[&[&[u8]]],
) -> ProgramResult {
    invoke_signed(
        &instruction_patched::init_open_orders(
            dex_program.key,
            open_orders.key,
            owner.key,
            market.key,
            None,
        )?,
        &[open_orders, owner, market],
        seeds,
    )
}

// v0.4.0 start use dynamic sysvars but keys still need to be passed
// Need to be reviewed before `serum-dex` update!
// https://github.com/project-serum/serum-dex/blob/v0.4.0/dex/src/instruction.rs#L909-L931
mod instruction_patched {
    use serum_dex::{error::DexError, instruction::MarketInstruction};
    use solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    pub fn init_open_orders(
        program_id: &Pubkey,
        open_orders: &Pubkey,
        owner: &Pubkey,
        market: &Pubkey,
        market_authority: Option<&Pubkey>,
    ) -> Result<Instruction, DexError> {
        let data = MarketInstruction::InitOpenOrders.pack();
        let mut accounts: Vec<AccountMeta> = vec![
            AccountMeta::new(*open_orders, false),
            AccountMeta::new_readonly(*owner, true),
            AccountMeta::new_readonly(*market, false),
            // AccountMeta::new_readonly(rent::ID, false),
            AccountMeta::new_readonly(*market, false),
        ];
        if let Some(market_authority) = market_authority {
            accounts.push(AccountMeta::new_readonly(*market_authority, true));
        }
        Ok(Instruction {
            program_id: *program_id,
            data,
            accounts,
        })
    }
}
