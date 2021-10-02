#![allow(dead_code)]
use anchor_lang::{InstructionData, ToAccountMetas};
use futures::future::{try_join, try_join_all};
use rand_chacha::{
    rand_core::{RngCore as _, SeedableRng as _},
    ChaCha20Rng,
};
use serum_dex::{
    instruction::{srm_token::ID as SerumTokenId, SelfTradeBehavior},
    matching::{OrderType, Side},
    state::{gen_vault_signer_key, OpenOrders},
};
use solana_program_test::{
    processor, tokio::sync::Mutex, BanksClient, ProgramTest, ProgramTestContext,
};
use solana_sdk::{
    account::Account,
    account_info::{Account as _, AccountInfo},
    entrypoint::ProgramResult,
    hash::hashv,
    instruction::Instruction,
    native_token::sol_to_lamports,
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signer,
    signer::{keypair::Keypair, signers::Signers},
    system_instruction, system_program,
    transaction::Transaction,
    transport::Result as TransportResult,
};
use spl_token::state::Mint as TokenMint;
use spl_token_client::{
    client::{TokenBanksClient, TokenBanksClientProcessTransaction, TokenClient},
    token::Token,
};
use std::{
    env::{current_dir, set_current_dir},
    mem::size_of,
    num::NonZeroU64,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex as SyncMutex},
};

pub type UtilError = Box<dyn std::error::Error>;
pub type UtilResult<T = ()> = Result<T, UtilError>;

lazy_static::lazy_static! {
    static ref SRM_TOKEN_DECIMALS: u8 = 6;
    static ref SRM_TOKEN_AUTHORITY: Keypair =
        Keypair::from_base58_string("4cizbpotMo3hC9GvMKG8yZYAQ1UACCVvQAoNQdc3y4zbKsm8frfDC2SdyjTiK8WRp626mWsKw94wudeo2TLvqXPE");
}

fn create_program_test() -> UtilResult<ProgramTest> {
    let mut pt = ProgramTest::default();

    // While in `ProgramTest::add_program` we add BPF and native, better test BPF.
    pt.prefer_bpf(true);

    // `serum_dex.so` not in `target/deploy`, so change cwd
    let cwd = current_dir()?;
    set_current_dir(into_serum_dex_deploy_dir(cwd.clone()))?;
    pt.add_program(
        "serum_dex",
        serum_saver::dex::ID,
        processor!(dex_process_instruction),
    );
    set_current_dir(cwd)?;

    pt.add_program(
        "serum_saver",
        serum_saver::ID,
        processor!(serum_saver::entry),
    );

    // Add accounts
    // let data = hex::decode("0000000059e4a73328f8a2075c5bb40bb3ce8a67d6640c33f3563e226a21c0efa50b7f68444eb3e08384230006010000000059e4a73328f8a2075c5bb40bb3ce8a67d6640c33f3563e226a21c0efa50b7f68").unwrap();
    // let state = TokenMint::unpack(&data).unwrap();
    // println!("{:#?}", state);
    let mut srm_token_account =
        Account::new(sol_to_lamports(1_000_000.0), TokenMint::LEN, &spl_token::ID);
    Pack::pack(
        TokenMint {
            mint_authority: COption::Some(SRM_TOKEN_AUTHORITY.pubkey()),
            supply: 0,
            decimals: *SRM_TOKEN_DECIMALS,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        &mut srm_token_account.data,
    )
    .expect("failed to pack srm_token");
    pt.add_account(SerumTokenId, srm_token_account);

    Ok(pt)
}

fn into_serum_dex_deploy_dir(mut current_dir: PathBuf) -> PathBuf {
    current_dir.pop();
    current_dir.pop();
    current_dir.push(PathBuf::from("contrib/serum-dex/dex/target/deploy"));
    current_dir
}

fn dex_process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    Ok(serum_dex::state::State::process(
        program_id,
        accounts,
        instruction_data,
    )?)
}

#[derive(Debug, Clone)]
pub struct SeedableRng(Arc<SyncMutex<ChaCha20Rng>>);

impl SeedableRng {
    pub fn from_seed(seed: &[&[u8]]) -> Self {
        let bytes = hashv(seed).to_bytes();
        Self(Arc::new(SyncMutex::new(ChaCha20Rng::from_seed(bytes))))
    }

    pub fn new_keypair(&self) -> Keypair {
        // ed25519_dalek from solana_sdk use old rand crate
        // Keypair::generate(&mut self.0)

        let mut seed = [0; 32];
        self.0.lock().unwrap().fill_bytes(&mut seed);

        use rand_chacha02::{rand_core::SeedableRng as _, ChaCha20Rng};
        let mut rng = ChaCha20Rng::from_seed(seed);

        Keypair::generate(&mut rng)
    }
}

fn keypair_clone(kp: &Keypair) -> Keypair {
    Keypair::from_bytes(&kp.to_bytes()).expect("failed to copy keypair")
}

pub async fn token_balance<S: Signer>(token: &TokenTestContext, owner: &S) -> UtilResult<u64> {
    let account = token.get_associated_token_address(&owner.pubkey());
    token_balance2(token, account).await
}

pub async fn token_balance2(token: &TokenTestContext, vault: Pubkey) -> UtilResult<u64> {
    Ok(token.get_account_info(vault).await?.amount)
}

/// Wrapper around `(Pubkey, Account)` for `AccountInfo`.
#[derive(Debug, Default, Clone)]
pub struct KeyedAccount {
    pub key: Pubkey,
    pub account: Account,
}

impl KeyedAccount {
    pub fn new(key: Pubkey, account: Account) -> Self {
        Self { key, account }
    }

    pub fn account_info(&mut self) -> AccountInfo {
        let is_signer = true;
        let is_writable = false;
        let (lamports, data, owner, executable, rent_epoch) = self.account.get();
        AccountInfo::new(
            &self.key,
            is_signer,
            is_writable,
            lamports,
            data,
            owner,
            executable,
            rent_epoch,
        )
    }
}

impl Deref for KeyedAccount {
    type Target = Account;

    fn deref(&self) -> &Self::Target {
        &self.account
    }
}

impl DerefMut for KeyedAccount {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.account
    }
}

async fn process_ixs<T: Signers>(
    banks_client: &mut BanksClient,
    instructions: &[Instruction],
    payer: &Pubkey,
    signing_keypairs: &T,
) -> TransportResult<()> {
    let recent_blockhash = banks_client.get_recent_blockhash().await?;
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(payer),
        signing_keypairs,
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await
}

type TokenTestContext = Token<TokenBanksClientProcessTransaction, Keypair>;

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct TestContext {
    pub rng: SeedableRng,
    #[derivative(Debug = "ignore")]
    pub ctx: Arc<Mutex<ProgramTestContext>>,
    pub payer: Keypair,

    pub srm_token: TokenTestContext,
    pub srm_token_decimals: u8,
    pub srm_token_authhority: Keypair,

    pub coin_token: TokenTestContext,
    pub coin_token_decimals: u8,
    pub coin_token_authority: Keypair,

    pub pc_token: TokenTestContext,
    pub pc_token_decimals: u8,
    pub pc_token_authority: Keypair,

    pub alice: Keypair,
    pub bob: Keypair,
    pub chuck: Keypair,
    pub david: Keypair,
}

impl TestContext {
    pub async fn new(log_level: Option<&str>) -> UtilResult<Arc<Self>> {
        const TOKEN_DECIMALS: u8 = 6;

        // SeedableRng, ProgramTest, Payer, Token client
        let rng = SeedableRng::from_seed(&["serum-saver".as_bytes()]);

        let pt = create_program_test()?;
        if let Some(filter) = log_level {
            solana_logger::setup_with_default(filter);
        }

        let ctx = pt.start_with_context().await;
        let ctx = Arc::new(Mutex::new(ctx));
        let payer = keypair_clone(&ctx.lock().await.payer);

        let client: Arc<dyn TokenClient<TokenBanksClientProcessTransaction>> =
            Arc::new(TokenBanksClient::new_from_context(
                Arc::clone(&ctx),
                TokenBanksClientProcessTransaction,
            ));

        // Serum Token
        let srm_token = Token::new(Arc::clone(&client), SerumTokenId, keypair_clone(&payer));

        // Coin Token
        let coin_token_authority = rng.new_keypair();
        let coin_token_authority_pubkey = coin_token_authority.pubkey();
        let coin_token_account = rng.new_keypair();
        let coin_token_decimals = TOKEN_DECIMALS;
        let coin_token = Token::create_mint(
            Arc::clone(&client),
            keypair_clone(&payer),
            &coin_token_account,
            &coin_token_authority_pubkey,
            None,
            coin_token_decimals,
        );

        // PC Token (price currency)
        let pc_token_authority = rng.new_keypair();
        let pc_token_authority_pubkey = pc_token_authority.pubkey();
        let pc_token_account = rng.new_keypair();
        let pc_token_decimals = TOKEN_DECIMALS;
        let pc_token = Token::create_mint(
            Arc::clone(&client),
            keypair_clone(&payer),
            &pc_token_account,
            &pc_token_authority_pubkey,
            None,
            pc_token_decimals,
        );

        // Wait futures
        let (coin_token, pc_token) = try_join(coin_token, pc_token).await?;

        // Users
        let alice = rng.new_keypair();
        let bob = rng.new_keypair();
        let chuck = rng.new_keypair();
        let david = rng.new_keypair();

        // Create Token addresses
        let mut futures = vec![];
        let mut ixs = vec![];
        for user in [&payer, &alice, &bob, &chuck, &david] {
            for token in [&coin_token, &pc_token] {
                futures.push(
                    async move { token.create_associated_token_account(&user.pubkey()).await },
                );
            }

            // Add some SOL
            ixs.push(system_instruction::transfer(
                &payer.pubkey(),
                &user.pubkey(),
                sol_to_lamports(10.0),
            ));
        }
        try_join_all(futures).await?;
        process_ixs(
            &mut ctx.lock().await.banks_client,
            &ixs,
            &payer.pubkey(),
            &[&payer],
        )
        .await?;

        Ok(Arc::new(Self {
            rng,
            ctx,
            payer,

            srm_token,
            srm_token_decimals: *SRM_TOKEN_DECIMALS,
            srm_token_authhority: keypair_clone(&SRM_TOKEN_AUTHORITY),

            coin_token,
            coin_token_decimals,
            coin_token_authority,

            pc_token,
            pc_token_decimals,
            pc_token_authority,

            alice,
            bob,
            chuck,
            david,
        }))
    }

    pub async fn get_account(&self, key: Pubkey) -> UtilResult<KeyedAccount> {
        let banks_client = &mut self.ctx.lock().await.banks_client;
        let account = banks_client.get_account(key).await?;
        Ok(KeyedAccount::new(key, account.expect("account not found")))
    }

    pub async fn gen_serum_dex(self: &Arc<Self>) -> UtilResult<Arc<TestContextDex>> {
        // Sizes from:
        // https://github.com/project-serum/serum-dex/blob/1f6d5867019e242a470deed79cddca0d1f15e0a3/dex/crank/src/lib.rs#L1276-L1280
        let payer_pubkey = self.payer.pubkey();
        let (market, market_ix) =
            TestContextDex::create_account_ix(&self.ctx, &self.rng, 376, &payer_pubkey).await?;
        let (request_queue, request_queue_ix) =
            TestContextDex::create_account_ix(&self.ctx, &self.rng, 640, &payer_pubkey).await?;
        let (event_queue, event_queue_ix) =
            TestContextDex::create_account_ix(&self.ctx, &self.rng, 1 << 20, &payer_pubkey).await?;
        let (bids, bids_ix) =
            TestContextDex::create_account_ix(&self.ctx, &self.rng, 1 << 16, &payer_pubkey).await?;
        let (asks, asks_ix) =
            TestContextDex::create_account_ix(&self.ctx, &self.rng, 1 << 16, &payer_pubkey).await?;
        process_ixs(
            &mut self.ctx.lock().await.banks_client,
            &[
                market_ix,
                request_queue_ix,
                event_queue_ix,
                bids_ix,
                asks_ix,
            ],
            &self.payer.pubkey(),
            &vec![
                &self.payer,
                &market,
                &request_queue,
                &event_queue,
                &bids,
                &asks,
            ],
        )
        .await?;

        let authority_pk = None;
        let prune_authority_pk = None;
        let (vault_signer_nonce, vault_signer) = {
            let mut i = 0;
            loop {
                assert!(i < 100);
                if let Ok(pk) = gen_vault_signer_key(i, &market.pubkey(), &serum_saver::dex::ID) {
                    break (i, pk);
                }
                i += 1;
            }
        };

        let (coin_vault, pc_vault) = try_join(
            self.coin_token
                .create_associated_token_account(&vault_signer),
            self.pc_token.create_associated_token_account(&vault_signer),
        )
        .await?;

        let coin_lot_size = 1_000;
        let coin_lots = u64::pow(10, self.coin_token_decimals as u32) / coin_lot_size;
        // While this is acceptable, for tests simplifying we disallow this
        assert!(
            u64::pow(10, self.coin_token_decimals as u32) % coin_lot_size == 0,
            "Tests are not allow lots remainder"
        );
        let pc_lot_size = 10;
        let pc_dust_threshold = 0;

        process_ixs(
            &mut self.ctx.lock().await.banks_client,
            &[serum_dex::instruction::initialize_market(
                &market.pubkey(),
                &serum_saver::dex::ID,
                self.coin_token.get_address(),
                self.pc_token.get_address(),
                &coin_vault,
                &pc_vault,
                authority_pk,
                prune_authority_pk,
                &bids.pubkey(),
                &asks.pubkey(),
                &request_queue.pubkey(),
                &event_queue.pubkey(),
                coin_lot_size,
                pc_lot_size,
                vault_signer_nonce,
                pc_dust_threshold,
            )?],
            &self.payer.pubkey(),
            &[&self.payer],
        )
        .await?;

        Ok(Arc::new(TestContextDex {
            tc: Arc::clone(self),

            market: market.pubkey(),
            request_queue: request_queue.pubkey(),
            event_queue: event_queue.pubkey(),
            bids: bids.pubkey(),
            asks: asks.pubkey(),
            vault_signer,
            vault_signer_nonce,
            coin_vault,
            pc_vault,
            coin_lot_size,
            coin_lots,
            pc_lot_size,
            pc_dust_threshold,
        }))
    }

    pub async fn gen_saver(self: &Arc<Self>) -> UtilResult<Arc<TestContextSaver>> {
        let saver = self.rng.new_keypair();
        let (signer, nonce) =
            Pubkey::find_program_address(&[saver.pubkey().as_ref()], &serum_saver::ID);

        let srm_token = &self.srm_token;
        let srm_vault = srm_token.create_associated_token_account(&signer).await?;

        process_ixs(
            &mut self.ctx.lock().await.banks_client,
            &[Instruction::new_with_bytes(
                serum_saver::ID,
                &serum_saver::instruction::InitializeSaver { nonce }.data(),
                serum_saver::accounts::InitializeSaver {
                    saver: saver.pubkey(),
                    signer,

                    srm_vault,

                    payer: self.payer.pubkey(),
                    system_program: system_program::id(),
                }
                .to_account_metas(None),
            )],
            &self.payer.pubkey(),
            &[&self.payer, &saver],
        )
        .await?;

        Ok(Arc::new(TestContextSaver {
            tc: Arc::clone(self),

            saver: saver.pubkey(),
            signer,
            srm_vault,
        }))
    }
}

#[derive(Debug)]
pub struct TestContextDex {
    pub tc: Arc<TestContext>,

    pub market: Pubkey,
    pub request_queue: Pubkey,
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub vault_signer: Pubkey,
    pub vault_signer_nonce: u64,
    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
    pub coin_lot_size: u64,
    pub coin_lots: u64,
    pub pc_lot_size: u64,
    pub pc_dust_threshold: u64,
}

impl TestContextDex {
    pub async fn create_account_ix(
        ctx: &Mutex<ProgramTestContext>,
        rng: &SeedableRng,
        unpadded_len: usize,
        payer: &Pubkey,
    ) -> UtilResult<(Keypair, Instruction)> {
        // padding is 12 bytes: `"serum" || data || "padding"`
        let len = unpadded_len + 12;

        let rent = ctx.lock().await.banks_client.get_rent().await?;
        let new_account = rng.new_keypair();
        let new_account_pubkey = new_account.pubkey();
        Ok((
            new_account,
            system_instruction::create_account(
                payer,
                &new_account_pubkey,
                rent.minimum_balance(len),
                len as u64,
                &serum_saver::dex::ID,
            ),
        ))
    }

    pub async fn gen_open_orders(&self, owner: &Keypair) -> UtilResult<Pubkey> {
        let (open_orders, open_orders_ix) = self.gen_open_orders_create().await?;
        self.gen_open_orders_init(&open_orders, open_orders_ix, owner)
            .await?;
        Ok(open_orders.pubkey())
    }

    async fn gen_open_orders_create(&self) -> UtilResult<(Keypair, Instruction)> {
        Self::create_account_ix(
            &self.tc.ctx,
            &self.tc.rng,
            size_of::<OpenOrders>(),
            &self.tc.payer.pubkey(),
        )
        .await
        .map_err(Into::into)
    }

    async fn gen_open_orders_init(
        &self,
        open_orders: &Keypair,
        open_orders_ix: Instruction,
        owner: &Keypair,
    ) -> UtilResult {
        process_ixs(
            &mut self.tc.ctx.lock().await.banks_client,
            &[
                open_orders_ix,
                serum_dex::instruction::init_open_orders(
                    &serum_saver::dex::ID,
                    &open_orders.pubkey(),
                    &owner.pubkey(),
                    &self.market,
                    None,
                )?,
            ],
            &self.tc.payer.pubkey(),
            &[&self.tc.payer, open_orders, owner],
        )
        .await?;
        Ok(())
    }

    pub async fn add_liquidity(
        &self,
        open_orders: &Pubkey,
        open_orders_owner: &Keypair,
        side: Side,
        limit_price: u64,
        coin_qty: u64,
    ) -> UtilResult<()> {
        let native_pc_qty_including_fees = coin_qty
            .checked_mul(self.pc_lot_size)
            .and_then(|value| value.checked_mul(limit_price))
            .and_then(NonZeroU64::new)
            .unwrap();

        let (token, authority, amount) = match side {
            Side::Bid => (
                &self.tc.pc_token,
                &self.tc.pc_token_authority,
                native_pc_qty_including_fees.get(),
            ),
            Side::Ask => (
                &self.tc.coin_token,
                &self.tc.coin_token_authority,
                coin_qty.checked_mul(self.coin_lot_size).unwrap(),
            ),
        };
        let order_payer = token.get_associated_token_address(&open_orders_owner.pubkey());
        // I do not know why `mint_to` is not work properly :\
        while token_balance2(token, order_payer).await? < amount {
            token.mint_to(&order_payer, authority, amount + 1).await?;
        }

        process_ixs(
            &mut self.tc.ctx.lock().await.banks_client,
            &[serum_dex::instruction::new_order(
                &self.market,
                open_orders,
                &self.request_queue,
                &self.event_queue,
                &self.bids,
                &self.asks,
                &order_payer,
                &open_orders_owner.pubkey(),
                &self.coin_vault,
                &self.pc_vault,
                &spl_token::ID,
                &spl_token::ID, // Should be `Rent::id()` but this is not used in v0.4.0
                None,           // srm_account_referral
                &serum_saver::dex::ID,
                side,
                NonZeroU64::new(limit_price).unwrap(),
                NonZeroU64::new(coin_qty).unwrap(),
                OrderType::Limit,
                0, // client_order_id
                SelfTradeBehavior::DecrementTake,
                u16::MAX, // limit
                native_pc_qty_including_fees,
            )?],
            &self.tc.payer.pubkey(),
            &[&self.tc.payer, open_orders_owner],
        )
        .await
        .map_err(Into::into)
    }

    pub async fn consume_events(&self, open_orders: Vec<&Pubkey>) -> UtilResult<()> {
        process_ixs(
            &mut self.tc.ctx.lock().await.banks_client,
            &[serum_dex::instruction::consume_events(
                &serum_saver::dex::ID,
                open_orders,
                &self.market,
                &self.event_queue,
                &spl_token::ID, // Not used
                &spl_token::ID, // Not used
                u16::MAX,
            )?],
            &self.tc.payer.pubkey(),
            &[&self.tc.payer],
        )
        .await
        .map_err(Into::into)
    }
}

#[derive(Debug)]
pub struct TestContextSaver {
    pub tc: Arc<TestContext>,

    pub saver: Pubkey,
    pub signer: Pubkey,
    pub srm_vault: Pubkey,
}

impl TestContextSaver {
    pub async fn gen_market(
        self: &Arc<Self>,
        dex: &Arc<TestContextDex>,
    ) -> UtilResult<TestContextSaverMarket> {
        if !Arc::ptr_eq(&self.tc, &dex.tc) {
            return Err("TestContextSaver.tc should be equal to TestContextDex.tc".into());
        }

        let saver_market = self.tc.rng.new_keypair();

        try_join(
            self.tc
                .coin_token
                .get_or_create_associated_account_info(&self.signer),
            self.tc
                .pc_token
                .get_or_create_associated_account_info(&self.signer),
        )
        .await?;

        let coin_vault = self
            .tc
            .coin_token
            .get_associated_token_address(&self.signer);
        let pc_vault = self.tc.pc_token.get_associated_token_address(&self.signer);

        let (open_orders, open_orders_ix) = dex.gen_open_orders_create().await?;

        process_ixs(
            &mut self.tc.ctx.lock().await.banks_client,
            &[
                open_orders_ix,
                Instruction::new_with_bytes(
                    serum_saver::ID,
                    &serum_saver::instruction::InitializeMarket {}.data(),
                    serum_saver::accounts::InitializeMarket {
                        saver_market: saver_market.pubkey(),

                        saver: self.saver,
                        signer: self.signer,

                        coin_mint: *self.tc.coin_token.get_address(),
                        coin_vault,
                        pc_mint: *self.tc.pc_token.get_address(),
                        pc_vault,

                        dex_program: serum_saver::dex::ID,
                        dex_market: dex.market,
                        dex_open_orders: open_orders.pubkey(),

                        payer: self.tc.payer.pubkey(),
                        system_program: system_program::id(),
                    }
                    .to_account_metas(None),
                ),
            ],
            &self.tc.payer.pubkey(),
            &[&self.tc.payer, &open_orders, &saver_market],
        )
        .await?;

        Ok(TestContextSaverMarket {
            tc: Arc::clone(&self.tc),
            dex: Arc::clone(dex),
            tcs: Arc::clone(self),

            saver_market: saver_market.pubkey(),
            open_orders: open_orders.pubkey(),
            coin_vault,
            pc_vault,
        })
    }
}

#[derive(Debug)]
pub struct TestContextSaverMarket {
    pub tc: Arc<TestContext>,
    pub dex: Arc<TestContextDex>,
    pub tcs: Arc<TestContextSaver>,

    pub saver_market: Pubkey,
    pub open_orders: Pubkey,
    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
}

impl TestContextSaverMarket {
    pub async fn buy(&self, limit_price: u64, max_coin_qty: u64, owner: &Keypair) -> UtilResult {
        self.swap(Side::Bid, limit_price, max_coin_qty, owner).await
    }

    pub async fn sell(&self, limit_price: u64, max_coin_qty: u64, owner: &Keypair) -> UtilResult {
        self.swap(Side::Ask, limit_price, max_coin_qty, owner).await
    }

    pub async fn swap(
        &self,
        side: Side,
        limit_price: u64,
        max_coin_qty: u64,
        owner: &Keypair,
    ) -> UtilResult {
        let max_native_pc_qty = limit_price * max_coin_qty * self.dex.pc_lot_size;
        let max_native_pc_qty_including_fees = ((max_native_pc_qty as f64) * 1.0022) as u64;

        let tc = &self.dex.tc;
        process_ixs(
            &mut self.dex.tc.ctx.lock().await.banks_client,
            &[Instruction::new_with_bytes(
                serum_saver::ID,
                &serum_saver::instruction::Swap {
                    side: side.into(),
                    limit_price,
                    max_coin_qty,
                    max_native_pc_qty_including_fees,
                }
                .data(),
                serum_saver::accounts::Swap {
                    saver: self.tcs.saver,
                    signer: self.tcs.signer,
                    srm_vault: self.tcs.srm_vault,

                    saver_market: self.saver_market,

                    coin_vault: self.coin_vault,
                    pc_vault: self.pc_vault,

                    coin_wallet: tc.coin_token.get_associated_token_address(&owner.pubkey()),
                    pc_wallet: tc.pc_token.get_associated_token_address(&owner.pubkey()),
                    wallet_signer: owner.pubkey(),

                    market: self.dex.market,
                    open_orders: self.open_orders,
                    request_queue: self.dex.request_queue,
                    event_queue: self.dex.event_queue,
                    bids: self.dex.bids,
                    asks: self.dex.asks,
                    dex_coin_vault: self.dex.coin_vault,
                    dex_pc_vault: self.dex.pc_vault,
                    dex_vault_signer: self.dex.vault_signer,

                    dex_program: serum_saver::dex::ID,
                    spl_token_program: serum_saver::token::ID,
                }
                .to_account_metas(None),
            )],
            &self.dex.tc.payer.pubkey(),
            &[&self.dex.tc.payer, owner],
        )
        .await?;

        Ok(())
    }
}
