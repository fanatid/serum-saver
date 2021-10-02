#![allow(unaligned_references)]

use serum_dex::matching::Side;
use solana_program_test::tokio;
use solana_sdk::signature::Signer;

use util::{token_balance, token_balance2, TestContext, UtilResult};

mod util;

#[tokio::test]
async fn playground() -> UtilResult<()> {
    // let tc = TestContext::new(None).await?;
    let tc = TestContext::new(Some("warn")).await?;
    let dex = tc.gen_serum_dex().await?;

    // Maker
    let open_orders_maker_key = dex.gen_open_orders(&tc.alice).await?;
    let (bid_price, ask_price) = (198, 202);
    dex.add_liquidity(
        &open_orders_maker_key,
        &tc.alice,
        Side::Bid,
        bid_price,
        100 * dex.coin_lots,
    )
    .await?;
    dex.add_liquidity(
        &open_orders_maker_key,
        &tc.alice,
        Side::Ask,
        ask_price,
        100 * dex.coin_lots,
    )
    .await?;

    let coin_vault = tc.coin_token.get_associated_token_address(&tc.bob.pubkey());
    tc.coin_token
        .mint_to(
            &coin_vault,
            &tc.coin_token_authority,
            1_000_000 * u64::pow(10, tc.coin_token_decimals as u32),
        )
        .await?;
    let pc_vault = tc.pc_token.get_associated_token_address(&tc.bob.pubkey());
    tc.pc_token
        .mint_to(
            &pc_vault,
            &tc.pc_token_authority,
            1_000_000 * u64::pow(10, tc.pc_token_decimals as u32),
        )
        .await?;

    let saver = tc.gen_saver().await?;
    // tc.srm_token
    //     .mint_to(
    //         &saver.srm_vault,
    //         &tc.srm_token_authhority,
    //         200 * u64::pow(10, tc.srm_token_decimals as u32),
    //     )
    //     .await?;
    let saver_market = saver.gen_market(&dex).await?;

    println!(
        "saver coin: {:?}",
        token_balance2(&tc.coin_token, saver_market.coin_vault).await?
    );
    println!(
        "saver pc: {:?}",
        token_balance2(&tc.pc_token, saver_market.pc_vault).await?
    );
    println!(
        "bob coin: {:?}",
        token_balance(&tc.coin_token, &tc.bob).await?
    );
    println!("bob pc: {:?}", token_balance(&tc.pc_token, &tc.bob).await?);
    saver_market.buy(204, dex.coin_lots, &tc.bob).await?;
    println!("swap");
    println!(
        "saver coin: {:?}",
        token_balance2(&tc.coin_token, saver_market.coin_vault).await?
    );
    println!(
        "saver pc: {:?}",
        token_balance2(&tc.pc_token, saver_market.pc_vault).await?
    );
    println!(
        "bob coin: {:?}",
        token_balance(&tc.coin_token, &tc.bob).await?
    );
    println!("bob pc: {:?}", token_balance(&tc.pc_token, &tc.bob).await?);
    saver_market.sell(196, dex.coin_lots, &tc.bob).await?;
    println!("swap");
    println!(
        "saver coin: {:?}",
        token_balance2(&tc.coin_token, saver_market.coin_vault).await?
    );
    println!(
        "saver pc: {:?}",
        token_balance2(&tc.pc_token, saver_market.pc_vault).await?
    );
    println!(
        "bob coin: {:?}",
        token_balance(&tc.coin_token, &tc.bob).await?
    );
    println!("bob pc: {:?}", token_balance(&tc.pc_token, &tc.bob).await?);

    // // Taker
    // let open_orders_taker_key = dex.gen_open_orders(&tc.bob).await?;
    // dex.make_swap(
    //     &open_orders_taker_key,
    //     &tc.bob,
    //     Side::Ask,
    //     100,
    //     dex.coin_lots,
    // )
    // .await?;

    // // Explore OpenOrders
    // let mut market_account = tc.get_account(dex.market).await?;
    // let market_account_info = market_account.account_info();
    // let market = Market::load(&market_account_info, &serum_saver::dex::ID)?;

    // let mut open_orders_maker_account = tc.get_account(open_orders_maker_key).await?;
    // let open_orders_maker_account_info = open_orders_maker_account.account_info();
    // let open_orders = market.load_orders_mut(
    //     &open_orders_maker_account_info,
    //     None,
    //     &serum_saver::dex::ID,
    //     None,
    //     None,
    // )?;

    // println!("maker open orders");
    // println!("native_coin_free {:?}", open_orders.native_coin_free);
    // println!("native_pc_free {:?}", open_orders.native_pc_free);
    // println!(
    //     "referrer_rebates_accrued {:?}",
    //     open_orders.referrer_rebates_accrued
    // );

    // let mut open_orders_taker_account = tc.get_account(open_orders_taker_key).await?;
    // let open_orders_taker_account_info = open_orders_taker_account.account_info();
    // let open_orders = market.load_orders_mut(
    //     &open_orders_taker_account_info,
    //     None,
    //     &serum_saver::dex::ID,
    //     None,
    //     None,
    // )?;

    // println!("taker open orders");
    // println!("native_coin_free {:?}", open_orders.native_coin_free);
    // println!("native_pc_free {:?}", open_orders.native_pc_free);
    // println!(
    //     "referrer_rebates_accrued {:?}",
    //     open_orders.referrer_rebates_accrued
    // );

    // println!("consume events");
    // dex.consume_events(vec![&open_orders_maker_key, &open_orders_taker_key])
    //     .await?;

    // let mut open_orders_maker_account = tc.get_account(open_orders_maker_key).await?;
    // let open_orders_maker_account_info = open_orders_maker_account.account_info();
    // let open_orders = market.load_orders_mut(
    //     &open_orders_maker_account_info,
    //     None,
    //     &serum_saver::dex::ID,
    //     None,
    //     None,
    // )?;

    // println!("maker open orders");
    // println!("native_coin_free {:?}", open_orders.native_coin_free);
    // println!("native_pc_free {:?}", open_orders.native_pc_free);
    // println!(
    //     "referrer_rebates_accrued {:?}",
    //     open_orders.referrer_rebates_accrued
    // );

    // let mut open_orders_taker_account = tc.get_account(open_orders_taker_key).await?;
    // let open_orders_taker_account_info = open_orders_taker_account.account_info();
    // let open_orders = market.load_orders_mut(
    //     &open_orders_taker_account_info,
    //     None,
    //     &serum_saver::dex::ID,
    //     None,
    //     None,
    // )?;

    // println!("taker open orders");
    // println!("native_coin_free {:?}", open_orders.native_coin_free);
    // println!("native_pc_free {:?}", open_orders.native_pc_free);
    // println!(
    //     "referrer_rebates_accrued {:?}",
    //     open_orders.referrer_rebates_accrued
    // );

    // println!("token balance");
    // println!(
    //     "bob coin token: {:?}",
    //     token_balance(&tc.coin_token, &tc.bob).await?
    // );
    // println!(
    //     "bob pc token: {:?}",
    //     token_balance(&tc.pc_token, &tc.bob).await?
    // );

    Ok(())
}
