use anchor_lang::prelude::*;
use serum_dex::{instruction::srm_token, matching::Side};
use solana_program::program::invoke_signed;
use spl_associated_token_account::get_associated_token_address as gata;
use std::num::NonZeroU64;
use {
    dex::{get_market_coin_lot_size, Dex, SideAnchor},
    error::{SaverError, SaverResult},
    token::{SplToken, TokenAccount, TokenAccountState, TokenMint},
};

pub mod dex;
pub mod error;
pub mod token;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod serum_saver {
    use super::*;

    pub fn initialize_saver(ctx: Context<InitializeSaver>, nonce: u8) -> SaverResult {
        ctx.accounts.handle(nonce)
    }

    pub fn initialize_market(ctx: Context<InitializeMarket>) -> SaverResult {
        ctx.accounts.handle()
    }

    pub fn swap(
        ctx: Context<Swap>,
        side: SideAnchor,
        limit_price: u64,
        max_coin_qty: u64,
        max_native_pc_qty_including_fees: u64,
    ) -> SaverResult {
        ctx.accounts.handle(
            side.into(),
            limit_price,
            max_coin_qty,
            max_native_pc_qty_including_fees,
        )
    }
}

#[account]
#[derive(Debug, Default, Copy)]
pub struct Saver {
    pub signer: Pubkey,
    pub nonce: u8,

    pub srm_vault: Pubkey,
}

#[derive(Accounts)]
#[instruction(nonce: u8)]
pub struct InitializeSaver<'info> {
    #[account(init, payer = payer)]
    pub saver: Box<Account<'info, Saver>>,
    #[account(seeds = [(*saver).as_ref().key.as_ref()], bump = nonce)]
    pub signer: AccountInfo<'info>,

    #[account(
        constraint = srm_vault.mint == srm_token::ID,
        constraint = srm_vault.owner == *signer.key,
        constraint = srm_vault.amount == 0,
        constraint = srm_vault.delegate.is_none(),
        constraint = srm_vault.state == TokenAccountState::Initialized,
        constraint = srm_vault.close_authority.is_none(),
        constraint = gata(signer.key, &srm_vault.mint) == srm_vault.key(),
    )]
    pub srm_vault: Box<Account<'info, TokenAccount>>,

    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeSaver<'info> {
    pub fn handle(&mut self, nonce: u8) -> SaverResult {
        let saver = &mut self.saver;

        saver.signer = self.signer.key();
        saver.nonce = nonce;

        saver.srm_vault = self.srm_vault.key();

        Ok(())
    }
}

#[account]
#[derive(Debug, Default, Copy)]
pub struct SaverMarket {
    pub saver: Pubkey,

    pub open_orders: Pubkey,
    pub coin_lot_size: u64,

    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
}

#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(init, payer = payer)]
    pub saver_market: Box<Account<'info, SaverMarket>>,

    #[account(has_one = signer)]
    pub saver: Box<Account<'info, Saver>>,
    pub signer: AccountInfo<'info>,

    pub coin_mint: Box<Account<'info, TokenMint>>,
    #[account(
        constraint = coin_vault.mint == coin_mint.key(),
        constraint = coin_vault.owner == saver.signer,
        constraint = coin_vault.amount == 0,
        constraint = coin_vault.delegate.is_none(),
        constraint = coin_vault.state == TokenAccountState::Initialized,
        constraint = coin_vault.close_authority.is_none(),
        constraint = gata(&saver.signer, &coin_vault.mint) == coin_vault.key(),
    )]
    pub coin_vault: Box<Account<'info, TokenAccount>>,

    pub pc_mint: Box<Account<'info, TokenMint>>,
    #[account(
        constraint = pc_vault.mint == pc_mint.key(),
        constraint = pc_vault.owner == saver.signer,
        // constraint = pc_vault.amount == 0,
        constraint = pc_vault.delegate.is_none(),
        constraint = pc_vault.state == TokenAccountState::Initialized,
        constraint = pc_vault.close_authority.is_none(),
        constraint = gata(&saver.signer, &pc_vault.mint) == pc_vault.key(),
    )]
    pub pc_vault: Box<Account<'info, TokenAccount>>,

    pub dex_program: Program<'info, Dex>,
    pub dex_market: AccountInfo<'info>,
    #[account(mut)]
    pub dex_open_orders: AccountInfo<'info>,

    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeMarket<'info> {
    pub fn handle(&mut self) -> SaverResult {
        self.initialize()?;
        self.init_open_orders()?;
        Ok(())
    }

    pub fn initialize(&mut self) -> SaverResult {
        let saver_market = &mut self.saver_market;

        saver_market.saver = self.saver.key();

        saver_market.open_orders = self.dex_open_orders.key();
        saver_market.coin_lot_size = get_market_coin_lot_size(&self.dex_market)?;

        saver_market.coin_vault = self.coin_vault.key();
        saver_market.pc_vault = self.pc_vault.key();

        Ok(())
    }

    fn init_open_orders(&self) -> ProgramResult {
        dex::init_open_orders(
            self.dex_program.to_account_info(),
            self.dex_open_orders.clone(),
            self.signer.clone(),
            self.dex_market.clone(),
            &[&[(*self.saver).as_ref().key.as_ref(), &[self.saver.nonce]]],
        )
    }
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(
        has_one = signer,
        has_one = srm_vault,
    )]
    pub saver: Box<Account<'info, Saver>>,
    pub signer: AccountInfo<'info>,
    // TODO: remove `mut` modifier
    // TODO: add own patched instruction: https://github.com/project-serum/serum-dex/pull/179
    #[account(mut)]
    pub srm_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        has_one = saver,
        has_one = open_orders,
        has_one = coin_vault,
        has_one = pc_vault,
    )]
    pub saver_market: Box<Account<'info, SaverMarket>>,

    #[account(mut)]
    pub coin_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub pc_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub coin_wallet: AccountInfo<'info>,
    #[account(mut)]
    pub pc_wallet: AccountInfo<'info>,
    pub wallet_signer: Signer<'info>,

    #[account(mut)]
    pub market: AccountInfo<'info>,
    #[account(mut)]
    pub open_orders: AccountInfo<'info>,
    #[account(mut)]
    pub request_queue: AccountInfo<'info>,
    #[account(mut)]
    pub event_queue: AccountInfo<'info>,
    #[account(mut)]
    pub bids: AccountInfo<'info>,
    #[account(mut)]
    pub asks: AccountInfo<'info>,
    #[account(mut)]
    pub dex_coin_vault: AccountInfo<'info>,
    #[account(mut)]
    pub dex_pc_vault: AccountInfo<'info>,
    pub dex_vault_signer: AccountInfo<'info>,

    pub dex_program: Program<'info, Dex>,
    pub spl_token_program: Program<'info, SplToken>,
}

impl<'info> Swap<'info> {
    pub fn handle(
        &mut self,
        side: Side,
        limit_price: u64,
        max_coin_qty: u64,
        max_native_pc_qty_including_fees: u64,
    ) -> SaverResult {
        let coin_balance = self.coin_vault.amount;
        let pc_balance = self.pc_vault.amount;

        let (take_from, take_to, take_amount) = match side {
            Side::Bid => (
                self.pc_wallet.clone(),
                self.pc_vault.to_account_info(),
                max_native_pc_qty_including_fees,
            ),
            Side::Ask => (
                self.coin_wallet.clone(),
                self.coin_vault.to_account_info(),
                max_coin_qty
                    .checked_mul(self.saver_market.coin_lot_size)
                    .ok_or(SaverError::CoinQtyOverflow)?,
            ),
        };

        token::transfer(
            take_from,
            take_to,
            self.wallet_signer.to_account_info(),
            take_amount,
            &[],
        )?;

        let seeds: &[&[&[u8]]] = &[&[(*self.saver).as_ref().key.as_ref(), &[self.saver.nonce]]];

        let order_payer = match side {
            Side::Bid => self.pc_vault.to_account_info(),
            Side::Ask => self.coin_vault.to_account_info(),
        };

        invoke_signed(
            &serum_dex::instruction::new_order(
                self.market.key,
                self.open_orders.key,
                self.request_queue.key,
                self.event_queue.key,
                self.bids.key,
                self.asks.key,
                order_payer.as_ref().key,
                self.signer.key,
                self.dex_coin_vault.key,
                self.dex_pc_vault.key,
                self.spl_token_program.key,
                self.spl_token_program.key, // Rent, but not used since v0.4.0
                Some((*self.srm_vault).as_ref().key),
                self.dex_program.key,
                side,
                NonZeroU64::new(limit_price).ok_or(SaverError::NonZeroU64)?,
                NonZeroU64::new(max_coin_qty).ok_or(SaverError::NonZeroU64)?,
                serum_dex::matching::OrderType::ImmediateOrCancel,
                0, // client_order_id
                serum_dex::instruction::SelfTradeBehavior::AbortTransaction,
                u16::MAX, // limit
                NonZeroU64::new(max_native_pc_qty_including_fees).ok_or(SaverError::NonZeroU64)?,
            )
            .map_err(|e| -> ProgramError { e.into() })?,
            &[
                self.market.clone(),
                self.open_orders.clone(),
                self.request_queue.clone(),
                self.event_queue.clone(),
                self.bids.clone(),
                self.asks.clone(),
                order_payer,
                self.signer.clone(),
                self.dex_coin_vault.clone(),
                self.dex_pc_vault.clone(),
                self.spl_token_program.to_account_info(),
                self.spl_token_program.to_account_info(), // Rent, but not used since v0.4.0
                self.srm_vault.to_account_info(),
            ],
            seeds,
        )?;

        invoke_signed(
            &serum_dex::instruction::settle_funds(
                self.dex_program.key,
                self.market.key,
                self.spl_token_program.key,
                self.open_orders.key,
                self.signer.key,
                self.dex_coin_vault.key,
                (*self.coin_vault).as_ref().key,
                self.dex_pc_vault.key,
                (*self.pc_vault).as_ref().key,
                Some(self.dex_pc_vault.key),
                self.dex_vault_signer.key,
            )
            .map_err(|e| -> ProgramError { e.into() })?,
            &[
                self.market.clone(),
                self.open_orders.clone(),
                self.signer.clone(),
                self.dex_coin_vault.clone(),
                self.dex_pc_vault.clone(),
                self.coin_vault.to_account_info(),
                self.pc_vault.to_account_info(),
                self.dex_vault_signer.clone(),
                self.spl_token_program.to_account_info(),
                self.dex_pc_vault.to_account_info(),
            ],
            seeds,
        )?;

        self.coin_vault.reload()?;
        let coin_balance_change = self.coin_vault.amount - coin_balance;
        if coin_balance_change > 0 {
            token::transfer(
                self.coin_vault.to_account_info(),
                self.coin_wallet.clone(),
                self.signer.clone(),
                coin_balance_change,
                seeds,
            )?;
        }

        self.pc_vault.reload()?;
        let pc_balance_change = self.pc_vault.amount - pc_balance;
        if pc_balance_change > 0 {
            token::transfer(
                self.pc_vault.to_account_info(),
                self.pc_wallet.clone(),
                self.signer.clone(),
                pc_balance_change,
                seeds,
            )?;
        }

        Ok(())
    }
}
